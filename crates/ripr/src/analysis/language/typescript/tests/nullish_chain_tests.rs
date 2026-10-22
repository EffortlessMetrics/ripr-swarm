//! RIPR-SPEC-0246 rule 5: an edit to a non-first `??` operand needs a tested
//! input that reaches it.

use super::*;

const OWNER: &str = "export function displayName(person) {\n  return person?.nick ?? person?.nick ?? \"anonymous\";\n}\n";
const OWNER_LINE: (usize, &str) = (2, "  return person?.nick ?? person?.nick ?? \"anonymous\";");
const OWNER_LINE_TEXT: &str = "  return person?.nick ?? person?.nick ?? \"anonymous\";";
const OLD_MIDDLE: &str = "  return person?.nick ?? person?.name ?? \"anonymous\";";

fn finding(label: &str, body: &str) -> Result<Finding, String> {
    let test = format!(
        "import {{ displayName }} from \"../src/settings\";\ntest(\"name\", () => {{\n  {body}\n}});\n"
    );
    nested_callback_tests::settings_finding_in(label, OWNER, OWNER_LINE, &test, OLD_MIDDLE)
}

#[test]
fn middle_operand_edit_needs_an_input_that_reaches_it() -> Result<(), String> {
    for (label, body, expected) in [
        // The reported shape: nick set, or no person at all.
        (
            "nullish-unreached",
            "expect(displayName({ nick: \"kit\", name: \"K\" })).toBe(\"kit\");\n  expect(displayName(undefined)).toBe(\"anonymous\");",
            ExposureClass::WeaklyExposed,
        ),
        // A person with only a name reaches the middle operand.
        (
            "nullish-name-only",
            "expect(displayName({ nick: \"kit\", name: \"K\" })).toBe(\"kit\");\n  expect(displayName({ name: \"K\" })).toBe(\"K\");",
            ExposureClass::Exposed,
        ),
        (
            "nullish-null-nick",
            "expect(displayName({ nick: null, name: \"K\" })).toBe(\"K\");",
            ExposureClass::Exposed,
        ),
        // An argument the reader cannot see keeps the ladder result.
        (
            "nullish-non-literal",
            "const p = load();\n  expect(displayName(p)).toBe(\"K\");",
            ExposureClass::Exposed,
        ),
        (
            "nullish-spread-input",
            "expect(displayName({ ...base })).toBe(\"K\");",
            ExposureClass::Exposed,
        ),
        // An empty person reaches the middle operand but both versions read
        // nullish there: still no discriminating input.
        (
            "nullish-empty-object",
            "expect(displayName({})).toBe(\"anonymous\");",
            ExposureClass::WeaklyExposed,
        ),
        // One reaching input among others is enough.
        (
            "nullish-mixed",
            "expect(displayName(undefined)).toBe(\"anonymous\");\n  expect(displayName({ name: \"K\" })).toBe(\"K\");",
            ExposureClass::Exposed,
        ),
        // A local binding is read like an inline object.
        (
            "nullish-local-name-only",
            "const p = { name: \"K\" };\n  expect(displayName(p)).toBe(\"K\");",
            ExposureClass::Exposed,
        ),
    ] {
        let finding = finding(label, body)?;
        assert_eq!(finding.class, expected, "{label}");
    }
    Ok(())
}

#[test]
fn first_operand_edit_is_unchanged() -> Result<(), String> {
    let owner = "export function displayName(person) {\n  return person?.name ?? person?.name ?? \"anonymous\";\n}\n";
    let test = "import { displayName } from \"../src/settings\";\ntest(\"name\", () => {\n  expect(displayName({ nick: \"kit\", name: \"K\" })).toBe(\"K\");\n  expect(displayName(undefined)).toBe(\"anonymous\");\n});\n";
    let finding = nested_callback_tests::settings_finding_in(
        "nullish-first-operand",
        owner,
        (2, "  return person?.name ?? person?.name ?? \"anonymous\";"),
        test,
        "  return person?.nick ?? person?.name ?? \"anonymous\";",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn chain_reader_refuses_what_it_cannot_parse() {
    let params = vec!["person".to_string()];
    let read = |old: &str, new: &str| NullishChainChange::read(old, new, &params);
    assert!(
        read(
            "return person?.nick ?? person?.name ?? \"x\";",
            "return person?.nick ?? person?.nick ?? \"x\";"
        )
        .is_some()
    );
    // `||` chains, calls, ternaries and first-operand edits fail open.
    assert!(
        read(
            "return person?.a || person?.b;",
            "return person?.a || person?.c;"
        )
        .is_none()
    );
    assert!(
        read(
            "return f(person) ?? person?.b;",
            "return f(person) ?? person?.c;"
        )
        .is_none()
    );
    assert!(
        read(
            "return person?.a ?? person?.b;",
            "return person?.c ?? person?.b;"
        )
        .is_none()
    );
    assert!(read("return person?.a ?? x?.b;", "return person?.a ?? x?.c;").is_none());
}

const UNREACHED: &str = "expect(displayName({ nick: \"kit\", name: \"K\" })).toBe(\"kit\");\n  expect(displayName(undefined)).toBe(\"anonymous\");";

fn finding_for(
    label: &str,
    owner: &str,
    line: &str,
    removed: &str,
    body: &str,
) -> Result<Finding, String> {
    let test = format!(
        "import {{ displayName }} from \"../src/settings\";\ntest(\"name\", () => {{\n  {body}\n}});\n"
    );
    let at = owner.lines().position(|l| l == line).map_or(2, |i| i + 1);
    nested_callback_tests::settings_finding_in(label, owner, (at, line), &test, removed)
}

#[test]
fn null_argument_reads_as_nullish() -> Result<(), String> {
    let finding = finding(
        "nullish-null-argument",
        "expect(displayName({ nick: \"kit\", name: \"K\" })).toBe(\"kit\");\n  expect(displayName(null)).toBe(\"anonymous\");",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn non_optional_read_of_undefined_fails_open() -> Result<(), String> {
    let line = "  return person.nick ?? person.nick ?? \"anonymous\";";
    let owner = format!("export function displayName(person) {{\n{line}\n}}\n");
    let finding = finding_for(
        "nullish-non-optional",
        &owner,
        line,
        "  return person.nick ?? person.name ?? \"anonymous\";",
        UNREACHED,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn rebound_parameter_fails_open() -> Result<(), String> {
    let owner = format!(
        "export function displayName(person) {{\n  person = normalize(person);\n{OWNER_LINE_TEXT}\n}}\n"
    );
    let finding = finding_for(
        "nullish-rebound",
        &owner,
        OWNER_LINE_TEXT,
        OLD_MIDDLE,
        UNREACHED,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn member_only_pin_keeps_the_ladder_result() -> Result<(), String> {
    let finding = finding(
        "nullish-member-pin",
        "expect(displayName({ nick: \"kit\" }).length).toBe(3);",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn or_chain_keeps_the_ladder_result() -> Result<(), String> {
    let line = "  return person?.nick || person?.nick || \"anonymous\";";
    let owner = format!("export function displayName(person) {{\n{line}\n}}\n");
    let finding = finding_for(
        "nullish-or-chain",
        &owner,
        line,
        "  return person?.nick || person?.name || \"anonymous\";",
        UNREACHED,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn final_operand_keeps_null_distinct_from_undefined() -> Result<(), String> {
    let line = "  return person?.a ?? person?.c;";
    let owner = format!("export function displayName(person) {{\n{line}\n}}\n");
    let finding = finding_for(
        "nullish-final-null",
        &owner,
        line,
        "  return person?.a ?? person?.b;",
        "expect(displayName({ a: null, b: null })).toBe(null);",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn defaulted_parameter_fails_open() -> Result<(), String> {
    let line = "  return person?.nick ?? person?.nick ?? \"anonymous\";";
    let owner = format!("export function displayName(person = {{ name: \"d\" }}) {{\n{line}\n}}\n");
    let finding = finding_for("nullish-defaulted", &owner, line, OLD_MIDDLE, UNREACHED)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn inherited_member_is_not_absent() -> Result<(), String> {
    let line = "  return person?.nick ?? person?.constructor ?? \"x\";";
    let owner = format!("export function displayName(person) {{\n{line}\n}}\n");
    let finding = finding_for(
        "nullish-inherited",
        &owner,
        line,
        "  return person?.nick ?? person?.name ?? \"x\";",
        "expect(displayName({})).toBe(\"x\");",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn default_parameter_reader() {
    assert!(parameter_may_have_default("function f(p = {}) {}", "p"));
    assert!(parameter_may_have_default(
        "const f = (a, p: T = x) => p",
        "p"
    ));
    assert!(!parameter_may_have_default(
        "function f(a = 1, p: T) {}",
        "p"
    ));
    assert!(!parameter_may_have_default(
        "function f(p: Map<string, number>) {}",
        "p"
    ));
}
