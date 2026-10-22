//! RIPR-SPEC-0246 rule 5: an edit to a non-first `??` operand needs a tested
//! input that reaches it.

use super::*;

const OWNER: &str = "export function displayName(person) {\n  return person?.nick ?? person?.nick ?? \"anonymous\";\n}\n";
const OWNER_LINE: (usize, &str) = (2, "  return person?.nick ?? person?.nick ?? \"anonymous\";");
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
