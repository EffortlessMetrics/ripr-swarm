use crate::domain::language_capability::{CapabilityFamily, family_established_for_any_framework};
use crate::domain::{ExposureClass, Finding, LanguageId, ProbeFamily, RelatedTest};
use serde_json::{Map, Value, json};
use std::path::{Component, Path};

struct NativePytestRoute<'a> {
    program: &'static str,
    args: [&'a str; 3],
    cwd: &'static str,
}

/// Build only from the producer's declared file and pytest node, never by
/// parsing an arbitrary user-authored shell command.
fn native_pytest_route(finding: &Finding) -> Option<NativePytestRoute<'_>> {
    if finding.language != Some(LanguageId::Python)
        || finding
            .evidence
            .iter()
            .any(|line| line.starts_with("suggested_test_line: "))
    {
        return None;
    }
    let unique = |prefix: &str| {
        let mut values = finding
            .evidence
            .iter()
            .filter_map(|s| s.strip_prefix(prefix));
        let value = values.next()?;
        (values.next().is_none() && !value.is_empty()).then_some(value)
    };
    let file = unique("suggested_test_file: ")?;
    let node = unique("suggested_test_node_id: ")?;
    let raw = unique("suggested_verify_command: ")?;
    let selected = crate::domain::python_suggested_related_test(finding)?;
    native_pytest_route_parts(file, node, raw, &selected.name, cfg!(windows), false)
}

fn native_pytest_route_parts<'a>(
    file: &str,
    node: &'a str,
    raw: &str,
    selected: &str,
    windows: bool,
    allow_qualified: bool,
) -> Option<NativePytestRoute<'a>> {
    let name = node.strip_prefix(file)?.strip_prefix("::")?;
    if !safe_qualified_name(&name.replace("::", "."))
        || (!allow_qualified && name.contains("::"))
        || name.rsplit("::").next()? != selected
        || crate::domain::python_pytest_verify_display(file, name, windows).as_deref() != Some(raw)
        || file.chars().chain(node.chars()).any(char::is_control)
        || Path::new(file)
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    Some(NativePytestRoute {
        program: "python",
        args: ["-m", "pytest", node],
        cwd: ".",
    })
}

fn safe_qualified_name(name: &str) -> bool {
    name.split('.').all(|part| {
        let mut chars = part.chars();
        matches!(chars.next(), Some(c) if c == '_' || identifier_start(c))
            && chars.all(|c| c == '_' || identifier_continue(c))
    })
}

#[cfg(any(feature = "lang-python", feature = "lang-rust"))]
fn identifier_start(c: char) -> bool {
    unicode_ident::is_xid_start(c)
}
#[cfg(any(feature = "lang-python", feature = "lang-rust"))]
fn identifier_continue(c: char) -> bool {
    unicode_ident::is_xid_continue(c)
}
#[cfg(not(any(feature = "lang-python", feature = "lang-rust")))]
fn identifier_start(c: char) -> bool {
    c.is_ascii_alphabetic()
}
#[cfg(not(any(feature = "lang-python", feature = "lang-rust")))]
fn identifier_continue(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

pub(crate) fn manual_python_raw(finding: &Finding) -> Option<&str> {
    if finding.language != Some(LanguageId::Python) {
        return None;
    }
    let unique = |prefix: &str| {
        let mut values = finding
            .evidence
            .iter()
            .filter_map(|s| s.strip_prefix(prefix));
        let value = values.next()?;
        (values.next().is_none() && !value.is_empty()).then_some(value)
    };
    let selected = crate::domain::python_suggested_related_test(finding)?;
    let file = unique("suggested_test_file: ")?;
    let raw = unique("suggested_verify_command: ")?;
    if raw.chars().any(char::is_control) {
        return None;
    }
    if let Some(node) = unique("suggested_test_node_id: ") {
        native_pytest_route_parts(file, node, raw, &selected.name, cfg!(windows), true)?;
        let name = node.strip_prefix(file)?.strip_prefix("::")?;
        if name.contains("::") {
            let qualified = unique("suggested_test_qualified_name: ")?;
            if !safe_qualified_name(qualified) || qualified.replace('.', "::") != name {
                return None;
            }
        }
    } else {
        let qualified = unique("suggested_test_qualified_name: ")?;
        if !safe_qualified_name(qualified)
            || qualified.rsplit('.').next()? != selected.name
            || file.chars().any(char::is_control)
            || Path::new(file)
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
            || crate::domain::python_unittest_verify_display(file, qualified, cfg!(windows))
                .as_deref()
                != Some(raw)
        {
            return None;
        }
    }
    Some(raw)
}

/// Render the producer's pytest selector as a Bash action at the selected root.
/// The compatibility command remains unchanged for framework/ledger consumers.
pub(crate) fn rooted_pytest_action(finding: &Finding, root: &Path) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let route = native_pytest_route(finding)?;
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(root)
    };
    let root = if route.cwd == "." {
        root
    } else {
        root.join(route.cwd)
    };
    let root = root.to_str()?;
    if root.chars().any(char::is_control) {
        return None;
    }
    let quote = |s: &str| crate::agent::loop_commands::shell_arg_for(s, false);
    Some(format!(
        "(cd -P -- {} && {} {} {} {})",
        quote(root),
        quote(route.program),
        quote(route.args[0]),
        quote(route.args[1]),
        quote(route.args[2])
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PythonRepairCard {
    pub(crate) card_version: String,
    pub(crate) source: String,
    pub(crate) canonical_gap_id: String,
    pub(crate) language: String,
    pub(crate) language_status: String,
    pub(crate) authority_boundary: String,
    pub(crate) repair_action: String,
    pub(crate) changed_owner: String,
    pub(crate) changed_behavior: String,
    pub(crate) current_test_evidence: String,
    pub(crate) missing_discriminator: String,
    pub(crate) recommended_test_shape: String,
    pub(crate) suggested_assertion: String,
    pub(crate) suggested_test_file: String,
    pub(crate) suggested_test_name: String,
    pub(crate) suggested_test_node_id: Option<String>,
    pub(crate) verify_command: String,
    pub(crate) verify_command_confidence: String,
    pub(crate) receipt_command: Option<String>,
    pub(crate) receipt_status: String,
    pub(crate) receipt_guidance: String,
    pub(crate) stop_conditions: Vec<String>,
    pub(crate) limits: Vec<String>,
}

pub(crate) fn python_repair_card(finding: &Finding) -> Option<PythonRepairCard> {
    if finding.language != Some(LanguageId::Python) || finding.class != ExposureClass::WeaklyExposed
    {
        return None;
    }
    // Capability gate (#5531): repo-mode Python findings are shaped like
    // diff-mode ones, but the capability table registers Python
    // `repair_routing` for diff mode only (absent repo cell = `not_proven`),
    // so a repo-scoped finding never becomes a repair card.
    let mode = finding.analysis_mode()?;
    if !family_established_for_any_framework(
        LanguageId::Python,
        mode,
        CapabilityFamily::RepairRouting,
    ) {
        return None;
    }
    // A deleted or moved line is base-side evidence, never a repair target
    // (RIPR-SPEC-0156, #7587): the line it names is gone from the candidate.
    if finding.source_currentness.is_base_side() {
        return None;
    }

    let gap = finding.canonical_gap.as_ref()?;
    let missing_discriminator = finding
        .activation
        .missing_discriminators
        .first()?
        .value
        .clone();
    let suggested_test_file = evidence_value(finding, "suggested_test_file: ")?.to_string();
    let suggested_test_name = evidence_value(finding, "suggested_test_name: ")?.to_string();
    let suggested_test_node_id =
        evidence_value(finding, "suggested_test_node_id: ").map(ToString::to_string);
    let verify_command = evidence_value(finding, "suggested_verify_command: ")?.to_string();
    let verify_command_confidence =
        evidence_value(finding, "suggested_verify_command_confidence: ")?.to_string();
    let stop_conditions = stop_conditions(
        &finding.probe.family,
        &missing_discriminator,
        &verify_command,
    );
    let repair_action = evidence_value(finding, "suggested_repair_action: ")
        .unwrap_or("add_or_strengthen_test")
        .to_string();
    // Placement already selects the first eligible runnable test. Keep the
    // card's evidence attached to that identity rather than ranking again.
    let related_test = crate::domain::python_suggested_related_test(finding)?;

    Some(PythonRepairCard {
        card_version: "python_repair_card.v1".to_string(),
        source: "check_python_preview".to_string(),
        canonical_gap_id: gap.id.clone(),
        language: "python".to_string(),
        language_status: "preview".to_string(),
        authority_boundary: "preview_advisory_only".to_string(),
        repair_action: repair_action.clone(),
        changed_owner: gap.owner.clone(),
        changed_behavior: changed_behavior(finding, gap.behavior_kind.as_str()),
        current_test_evidence: current_test_evidence(related_test),
        missing_discriminator: missing_discriminator.clone(),
        recommended_test_shape: recommended_test_shape(
            &finding.probe.family,
            &missing_discriminator,
            &verify_command,
            &repair_action,
        ),
        suggested_assertion: suggested_assertion(
            &finding.probe.family,
            &missing_discriminator,
            &verify_command,
        ),
        suggested_test_file,
        suggested_test_name,
        suggested_test_node_id,
        verify_command,
        verify_command_confidence,
        receipt_command: None,
        receipt_status: "unavailable_until_python_gap_ledger".to_string(),
        receipt_guidance: receipt_guidance(),
        stop_conditions,
        limits: limits(),
    })
}

pub(crate) fn python_repair_card_json_value(card: &PythonRepairCard) -> Value {
    let mut suggested_location = Map::new();
    suggested_location.insert(
        "test_file".to_string(),
        json!(card.suggested_test_file.as_str()),
    );
    suggested_location.insert(
        "test_name".to_string(),
        json!(card.suggested_test_name.as_str()),
    );
    if let Some(node_id) = &card.suggested_test_node_id {
        suggested_location.insert("pytest_node_id".to_string(), json!(node_id));
    }

    json!({
        "card_version": card.card_version.as_str(),
        "source": card.source.as_str(),
        "canonical_gap_id": card.canonical_gap_id.as_str(),
        "language": card.language.as_str(),
        "language_status": card.language_status.as_str(),
        "authority_boundary": card.authority_boundary.as_str(),
        "repair_action": card.repair_action.as_str(),
        "changed_owner": card.changed_owner.as_str(),
        "changed_behavior": card.changed_behavior.as_str(),
        "current_test_evidence": card.current_test_evidence.as_str(),
        "missing_discriminator": card.missing_discriminator.as_str(),
        "recommended_test_shape": card.recommended_test_shape.as_str(),
        "suggested_assertion": card.suggested_assertion.as_str(),
        "suggested_location": Value::Object(suggested_location),
        "verify": {
            "command": card.verify_command.as_str(),
            "confidence": card.verify_command_confidence.as_str()
        },
        "receipt": {
            "command": card.receipt_command.as_deref(),
            "status": card.receipt_status.as_str(),
            "guidance": card.receipt_guidance.as_str()
        },
        "stop_conditions": &card.stop_conditions,
        "limits": &card.limits
    })
}

fn changed_behavior(finding: &Finding, behavior_kind: &str) -> String {
    let expression = finding
        .probe
        .after
        .as_deref()
        .unwrap_or(finding.probe.expression.as_str())
        .trim();
    format!(
        "{behavior_kind} changed at {}:{}: `{expression}`",
        crate::output::path::display_path(&finding.probe.location.file),
        finding.probe.location.line
    )
}

fn current_test_evidence(test: &RelatedTest) -> String {
    let oracle = test
        .oracle
        .as_deref()
        .map(|value| format!(": {value}"))
        .unwrap_or_default();
    format!(
        "{}:{} {} currently has oracle_strength={}, oracle_kind={}{}",
        crate::output::path::display_path(&test.file),
        test.line,
        test.name,
        test.oracle_strength.as_str(),
        test.oracle_kind.as_str(),
        oracle
    )
}

fn recommended_test_shape(
    family: &ProbeFamily,
    missing_discriminator: &str,
    verify_command: &str,
    repair_action: &str,
) -> String {
    let framework = framework_label(verify_command);
    let verb = if repair_action == "strengthen_existing_test" {
        "Strengthen the existing"
    } else {
        "Add or strengthen a"
    };
    match family {
        ProbeFamily::Predicate => {
            if let Some(candidate) =
                pytest_boundary_parametrization(missing_discriminator, verify_command)
            {
                return format!(
                    "{verb} {framework} boundary assertion for `{missing_discriminator}`. Keep the equality case as the minimum repair; optional pytest parameterization can add `{}`, `{}`, and `{}` rows when expected values are clear.",
                    candidate.below, candidate.equal_value, candidate.above
                );
            }
            format!("{verb} {framework} boundary assertion for `{missing_discriminator}`.")
        }
        ProbeFamily::ReturnValue => {
            format!(
                "{verb} {framework} exact return-value assertion for `{missing_discriminator}`."
            )
        }
        ProbeFamily::ErrorPath => {
            format!("{verb} {framework} exception assertion for `{missing_discriminator}`.")
        }
        ProbeFamily::FieldConstruction => {
            format!(
                "{verb} {framework} {} for `{missing_discriminator}`.",
                field_assertion_label(missing_discriminator)
            )
        }
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
            if missing_discriminator.starts_with("exit_code == ") {
                return format!(
                    "{verb} {framework} CLI exit-code assertion for `{missing_discriminator}`."
                );
            }
            if missing_discriminator.starts_with("stdout contains ")
                || missing_discriminator.starts_with("stderr contains ")
                || missing_discriminator.starts_with("output contains ")
            {
                return format!(
                    "{verb} {framework} CLI output assertion for `{missing_discriminator}`."
                );
            }
            format!(
                "{verb} {framework} output/log/call-effect assertion for `{missing_discriminator}`."
            )
        }
        _ => format!("{verb} {framework} focused assertion for `{missing_discriminator}`."),
    }
}

fn suggested_assertion(
    family: &ProbeFamily,
    missing_discriminator: &str,
    verify_command: &str,
) -> String {
    match family {
        ProbeFamily::Predicate => {
            predicate_boundary_assertion(missing_discriminator, verify_command)
        }
        ProbeFamily::ReturnValue => {
            "Assert the returned value equals the expected value for the changed inputs."
                .to_string()
        }
        ProbeFamily::ErrorPath if verify_command.starts_with("python -m unittest ") => {
            unittest_exception_assertion(missing_discriminator)
        }
        ProbeFamily::ErrorPath => pytest_exception_assertion(missing_discriminator),
        ProbeFamily::FieldConstruction => field_assertion(missing_discriminator),
        ProbeFamily::SideEffect | ProbeFamily::CallDeletion => {
            if missing_discriminator.starts_with("exit_code == ") {
                return format!("Assert the CLI exit code satisfies `{missing_discriminator}`.");
            }
            if missing_discriminator.starts_with("stdout contains ")
                || missing_discriminator.starts_with("stderr contains ")
                || missing_discriminator.starts_with("output contains ")
            {
                return format!("Assert the CLI output satisfies `{missing_discriminator}`.");
            }
            format!(
                "Assert the changed output, log text, or call effect for `{missing_discriminator}`."
            )
        }
        _ => format!("Assert the changed behavior for `{missing_discriminator}`."),
    }
}

fn predicate_boundary_assertion(missing_discriminator: &str, verify_command: &str) -> String {
    if let Some(candidate) = pytest_boundary_parametrization(missing_discriminator, verify_command)
    {
        return format!(
            "Assert the owner result or effect at `{}` first. Optional pytest shape: @pytest.mark.parametrize(\"{}, expected\", [({}, ...), ({}, ...), ({}, ...)]); fill expected values from domain behavior only.",
            candidate.equal,
            candidate.input_name,
            candidate.below,
            candidate.equal_value,
            candidate.above
        );
    }
    format!("Assert the owner result or effect at the boundary `{missing_discriminator}`.")
}

fn field_assertion_label(missing_discriminator: &str) -> &'static str {
    if missing_discriminator.starts_with("response.json()[") {
        return "response JSON field assertion";
    }
    if missing_discriminator.starts_with("response.status_code == ") {
        return "response status-code assertion";
    }
    if let Some((lhs, _rhs)) = split_equality_discriminator(missing_discriminator) {
        if is_python_identifier(lhs) {
            return "returned mapping field assertion";
        }
        if lhs.starts_with("result.") {
            return "returned object field assertion";
        }
        if lhs.contains('.') {
            return "object field assertion";
        }
    }
    "field/object assertion"
}

fn field_assertion(missing_discriminator: &str) -> String {
    if missing_discriminator.starts_with("response.json()[")
        || missing_discriminator.starts_with("response.status_code == ")
    {
        return format!("Assert the response field directly: `assert {missing_discriminator}`.");
    }
    if let Some((lhs, rhs)) = split_equality_discriminator(missing_discriminator) {
        if is_python_identifier(lhs) {
            return format!(
                "Assert the returned mapping field directly, e.g. `assert result[{lhs:?}] == {rhs}`."
            );
        }
        if let Some(field) = lhs.strip_prefix("self.") {
            return format!(
                "Assert the observed instance field directly, e.g. `assert <instance>.{field} == {rhs}`."
            );
        }
        if lhs.contains('.') {
            return format!("Assert the object field directly: `assert {missing_discriminator}`.");
        }
    }
    format!("Assert the returned object or field satisfies `{missing_discriminator}`.")
}

fn pytest_exception_assertion(missing_discriminator: &str) -> String {
    if let Some((exception, message)) = parse_exception_discriminator(missing_discriminator) {
        return format!("with pytest.raises({exception}, match={message:?}): ...");
    }
    "with pytest.raises(<expected exception>): ...".to_string()
}

fn split_equality_discriminator(value: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = value.split_once(" == ")?;
    let lhs = lhs.trim();
    let rhs = rhs.trim();
    (!lhs.is_empty() && !rhs.is_empty()).then_some((lhs, rhs))
}

fn unittest_exception_assertion(missing_discriminator: &str) -> String {
    if let Some((exception, message)) = parse_exception_discriminator(missing_discriminator) {
        return format!("with self.assertRaisesRegex({exception}, {message:?}): ...");
    }
    "with self.assertRaises(<expected exception>): ...".to_string()
}

fn parse_exception_discriminator(value: &str) -> Option<(&str, String)> {
    let rest = value.strip_prefix("raises ")?;
    let (exception, message) = rest.split_once(" matching ")?;
    let message = message.trim().trim_matches('"').to_string();
    if exception.trim().is_empty() || message.is_empty() {
        return None;
    }
    Some((exception.trim(), message))
}

fn framework_label(verify_command: &str) -> &'static str {
    if verify_command.starts_with("python -m unittest ") {
        "unittest"
    } else if crate::domain::is_pytest_verify_command(verify_command) {
        "pytest"
    } else {
        "Python"
    }
}

fn stop_conditions(
    family: &ProbeFamily,
    missing_discriminator: &str,
    verify_command: &str,
) -> Vec<String> {
    let mut conditions = vec![
        "Stop if imports, fixtures, or test setup cannot call the changed owner.".to_string(),
        "Stop if the expected value for the missing discriminator is ambiguous.".to_string(),
        "Stop if adding the test appears to require a production-code edit.".to_string(),
    ];
    if matches!(family, ProbeFamily::Predicate)
        && pytest_boundary_parametrization(missing_discriminator, verify_command).is_some()
    {
        conditions.push(
            "Stop before adding parametrized below/above rows if their expected values are not clear; keep only the equality-boundary assertion.".to_string(),
        );
    }
    if matches!(family, ProbeFamily::FieldConstruction)
        && missing_discriminator.starts_with("result.")
    {
        conditions.push(
            "Stop if the returned object does not expose the constructor keyword as a public field or attribute.".to_string(),
        );
    }
    conditions
}

fn limits() -> Vec<String> {
    vec![
        "Syntax-first Python preview evidence only.".to_string(),
        "No source edits, generated tests, mutation execution, provider calls, or gate authority."
            .to_string(),
        "Verify success alone is not a gap-closure receipt.".to_string(),
    ]
}

fn receipt_guidance() -> String {
    "Save this `ripr check --format json` report, then run `ripr first-pr --check-output <check.json>` or `ripr reports gap-ledger --check-output <check.json>` to materialize a gap ledger with a concrete receipt command.".to_string()
}

fn evidence_value<'a>(finding: &'a Finding, prefix: &str) -> Option<&'a str> {
    finding
        .evidence
        .iter()
        .find_map(|entry| entry.strip_prefix(prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PytestBoundaryParametrization {
    input_name: String,
    equal_value: String,
    below: String,
    equal: String,
    above: String,
}

fn pytest_boundary_parametrization(
    missing_discriminator: &str,
    verify_command: &str,
) -> Option<PytestBoundaryParametrization> {
    if !crate::domain::is_pytest_verify_command(verify_command) {
        return None;
    }
    let (input, boundary) = missing_discriminator.split_once(" == ")?;
    let input = input.trim();
    let boundary = boundary.trim();
    if !is_python_identifier(input) || !is_boundary_value_candidate(boundary) {
        return None;
    }
    let (below, equal_value, above) = boundary_candidate_values(boundary)?;
    Some(PytestBoundaryParametrization {
        input_name: input.to_string(),
        equal: format!("{input} == {equal_value}"),
        equal_value,
        below,
        above,
    })
}

fn is_boundary_value_candidate(value: &str) -> bool {
    value.parse::<i64>().is_ok() || is_python_identifier(value)
}

fn boundary_candidate_values(value: &str) -> Option<(String, String, String)> {
    if let Ok(number) = value.parse::<i64>() {
        // Python integers are not limited to i64. Widen before finding the
        // neighbors so an endpoint does not duplicate the equality row.
        let number = i128::from(number);
        return Some((
            (number - 1).to_string(),
            number.to_string(),
            (number + 1).to_string(),
        ));
    }
    is_python_identifier(value).then(|| {
        (
            format!("{value} - 1"),
            value.to_string(),
            format!("{value} + 1"),
        )
    })
}

fn is_python_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_exception_discriminator, pytest_boundary_parametrization, pytest_exception_assertion,
        recommended_test_shape, stop_conditions, suggested_assertion,
    };
    use crate::domain::ProbeFamily;

    #[test]
    fn native_route_refuses_disagreeing_or_escaping_test_identity() {
        let route = super::native_pytest_route_parts;
        let file = "tests/test_order.py";
        let node = "tests/test_order.py::test_total";
        let raw = "python -m pytest tests/test_order.py::test_total";
        #[cfg(any(feature = "lang-python", feature = "lang-rust"))]
        assert!(
            route(
                "tests/test_order.py",
                "tests/test_order.py::test_café",
                "python -m pytest tests/test_order.py::test_café",
                "test_café",
                false,
                false
            )
            .is_some()
        );
        #[cfg(not(any(feature = "lang-python", feature = "lang-rust")))]
        assert!(
            route(
                "tests/test_order.py",
                "tests/test_order.py::test_café",
                "python -m pytest tests/test_order.py::test_café",
                "test_café",
                false,
                false
            )
            .is_none()
        );
        assert!(
            route(
                file,
                "tests/test_order.py::$(touch x)::test_total",
                "python -m pytest tests/test_order.py::$(touch x)::test_total",
                "test_total",
                false,
                true
            )
            .is_none()
        );
        assert!(route(file, node, raw, "test_total", false, false).is_some());
        assert!(route(file, node, raw, "test_other", false, false).is_none());
        assert!(
            route(
                file,
                node,
                "python -m pytest tests/test_order.py::test_other",
                "test_total",
                false,
                false
            )
            .is_none()
        );
        assert!(
            route(
                file,
                "tests/test_other.py::test_total",
                raw,
                "test_total",
                false,
                false
            )
            .is_none()
        );
        assert!(
            route(
                "../test_order.py",
                "../test_order.py::test_total",
                "python -m pytest ../test_order.py::test_total",
                "test_total",
                false,
                false
            )
            .is_none()
        );
        let class_node = "tests/test_order.py::TestOther::test_total";
        let class_raw = "python -m pytest tests/test_order.py::TestOther::test_total";
        assert!(route(file, class_node, class_raw, "test_total", false, false).is_none());
        assert!(route(file, class_node, class_raw, "test_total", false, true).is_some());
        assert!(
            route(
                "tests/my file.py",
                "tests/my file.py::test_total",
                r#"python -m pytest "tests/my file.py::test_total""#,
                "test_total",
                true,
                true
            )
            .is_some()
        );
    }

    #[test]
    fn exception_assertion_uses_matching_message_when_available() {
        assert_eq!(
            pytest_exception_assertion("raises ValueError matching \"positive required\""),
            "with pytest.raises(ValueError, match=\"positive required\"): ..."
        );
    }

    #[test]
    fn exception_discriminator_parse_rejects_non_exception_text() {
        assert!(parse_exception_discriminator("amount == threshold").is_none());
    }

    #[test]
    fn side_effect_test_shape_specializes_cli_exit_and_output_cards() {
        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::SideEffect,
                "exit_code == 2",
                "pytest tests/test_cli.py::test_cli_smoke",
                "strengthen_existing_test",
            ),
            "Strengthen the existing pytest CLI exit-code assertion for `exit_code == 2`."
        );
        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::SideEffect,
                "output contains \"shipment queued\"",
                "pytest tests/test_cli.py::test_cli_smoke",
                "strengthen_existing_test",
            ),
            "Strengthen the existing pytest CLI output assertion for `output contains \"shipment queued\"`."
        );
        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::CallDeletion,
                "log contains \"shipment queued\"",
                "python -m unittest tests.test_cli.TestCli.test_cli_smoke",
                "add_or_strengthen_test",
            ),
            "Add or strengthen a unittest output/log/call-effect assertion for `log contains \"shipment queued\"`."
        );
    }

    #[test]
    fn field_cards_suggest_direct_assertion_shapes() {
        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::FieldConstruction,
                "status == \"paid\"",
                "pytest tests/test_invoice.py::test_invoice_payload_smoke",
                "strengthen_existing_test",
            ),
            "Strengthen the existing pytest returned mapping field assertion for `status == \"paid\"`."
        );
        assert_eq!(
            suggested_assertion(
                &ProbeFamily::FieldConstruction,
                "status == \"paid\"",
                "pytest tests/test_invoice.py::test_invoice_payload_smoke",
            ),
            "Assert the returned mapping field directly, e.g. `assert result[\"status\"] == \"paid\"`."
        );
        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::FieldConstruction,
                "result.active == True",
                "pytest tests/test_users.py::test_build_user_smoke",
                "strengthen_existing_test",
            ),
            "Strengthen the existing pytest returned object field assertion for `result.active == True`."
        );
        assert_eq!(
            suggested_assertion(
                &ProbeFamily::FieldConstruction,
                "result.active == True",
                "pytest tests/test_users.py::test_build_user_smoke",
            ),
            "Assert the object field directly: `assert result.active == True`."
        );
        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::FieldConstruction,
                "response.json()[\"detail\"] == \"coupon expired\"",
                "pytest tests/test_checkout.py::test_expired_coupon_response_smoke",
                "strengthen_existing_test",
            ),
            "Strengthen the existing pytest response JSON field assertion for `response.json()[\"detail\"] == \"coupon expired\"`."
        );
        assert_eq!(
            suggested_assertion(
                &ProbeFamily::FieldConstruction,
                "response.status_code == 422",
                "pytest tests/test_checkout.py::test_expired_coupon_response_smoke",
            ),
            "Assert the response field directly: `assert response.status_code == 422`."
        );
    }

    #[test]
    fn field_cards_cover_response_object_and_fallback_assertion_shapes() {
        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::FieldConstruction,
                "response.status_code == 422",
                "pytest tests/test_checkout.py::test_expired_coupon_response_smoke",
                "strengthen_existing_test",
            ),
            "Strengthen the existing pytest response status-code assertion for `response.status_code == 422`."
        );
        assert_eq!(
            suggested_assertion(
                &ProbeFamily::FieldConstruction,
                "response.json()[\"detail\"] == \"coupon expired\"",
                "pytest tests/test_checkout.py::test_expired_coupon_response_smoke",
            ),
            "Assert the response field directly: `assert response.json()[\"detail\"] == \"coupon expired\"`."
        );
        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::FieldConstruction,
                "order.total == 42",
                "pytest tests/test_order.py::test_total_smoke",
                "strengthen_existing_test",
            ),
            "Strengthen the existing pytest object field assertion for `order.total == 42`."
        );
        assert_eq!(
            suggested_assertion(
                &ProbeFamily::FieldConstruction,
                "self.total == 42",
                "pytest tests/test_order.py::test_total_smoke",
            ),
            "Assert the observed instance field directly, e.g. `assert <instance>.total == 42`."
        );
        assert_eq!(
            suggested_assertion(
                &ProbeFamily::FieldConstruction,
                "order.total == 42",
                "pytest tests/test_order.py::test_total_smoke",
            ),
            "Assert the object field directly: `assert order.total == 42`."
        );
        assert_eq!(
            suggested_assertion(
                &ProbeFamily::FieldConstruction,
                "payload contains expected detail",
                "pytest tests/test_order.py::test_total_smoke",
            ),
            "Assert the returned object or field satisfies `payload contains expected detail`."
        );
    }

    #[test]
    fn pytest_boundary_cards_suggest_parametrized_rows_without_expected_values() {
        let candidate = pytest_boundary_parametrization(
            "amount == threshold",
            "pytest tests/test_discount.py::test_apply_discount_smoke",
        );
        assert_eq!(
            candidate
                .as_ref()
                .map(|candidate| candidate.input_name.as_str()),
            Some("amount")
        );
        assert_eq!(
            candidate.as_ref().map(|candidate| candidate.below.as_str()),
            Some("threshold - 1")
        );
        assert_eq!(
            candidate.as_ref().map(|candidate| candidate.equal.as_str()),
            Some("amount == threshold")
        );
        assert_eq!(
            candidate.as_ref().map(|candidate| candidate.above.as_str()),
            Some("threshold + 1")
        );

        assert_eq!(
            recommended_test_shape(
                &ProbeFamily::Predicate,
                "amount == threshold",
                "pytest tests/test_discount.py::test_apply_discount_smoke",
                "strengthen_existing_test",
            ),
            "Strengthen the existing pytest boundary assertion for `amount == threshold`. Keep the equality case as the minimum repair; optional pytest parameterization can add `threshold - 1`, `threshold`, and `threshold + 1` rows when expected values are clear."
        );
        assert_eq!(
            suggested_assertion(
                &ProbeFamily::Predicate,
                "amount == threshold",
                "pytest tests/test_discount.py::test_apply_discount_smoke",
            ),
            "Assert the owner result or effect at `amount == threshold` first. Optional pytest shape: @pytest.mark.parametrize(\"amount, expected\", [(threshold - 1, ...), (threshold, ...), (threshold + 1, ...)]); fill expected values from domain behavior only."
        );
        assert!(
            stop_conditions(
                &ProbeFamily::Predicate,
                "amount == threshold",
                "pytest tests/test_discount.py::test_apply_discount_smoke",
            )
            .iter()
            .any(|condition| condition.contains("below/above rows"))
        );
        assert!(
            stop_conditions(
                &ProbeFamily::FieldConstruction,
                "result.active == True",
                "pytest tests/test_users.py::test_build_user_smoke",
            )
            .iter()
            .any(|condition| condition.contains("constructor keyword"))
        );
    }

    #[test]
    fn pytest_boundary_parametrization_fails_closed_for_unclear_values() {
        assert!(
            pytest_boundary_parametrization(
                "amount == calculate_threshold()",
                "pytest tests/test_discount.py::test_apply_discount_smoke",
            )
            .is_none()
        );
        assert!(
            pytest_boundary_parametrization(
                "amount == threshold",
                "python -m unittest tests.test_discount.TestDiscount.test_boundary",
            )
            .is_none()
        );
    }

    #[test]
    fn pytest_boundary_rows_remain_distinct_at_i64_limits() -> Result<(), String> {
        for (boundary, below, above) in [
            ("20", "19", "21"),
            (
                "9223372036854775807",
                "9223372036854775806",
                "9223372036854775808",
            ),
            (
                "-9223372036854775808",
                "-9223372036854775809",
                "-9223372036854775807",
            ),
        ] {
            let discriminator = format!("units == {boundary}");
            let command = "python -m pytest tests/test_pricing.py::test_price";
            let candidate = pytest_boundary_parametrization(&discriminator, command)
                .ok_or_else(|| format!("no pytest guidance for {discriminator}"))?;
            assert_eq!(candidate.below, below);
            assert_eq!(candidate.equal_value, boundary);
            assert_eq!(candidate.above, above);
            let rendered = suggested_assertion(&ProbeFamily::Predicate, &discriminator, command);
            assert!(
                rendered.contains(&format!(
                    "[({below}, ...), ({boundary}, ...), ({above}, ...)]"
                )),
                "{rendered}"
            );
        }
        Ok(())
    }
}
