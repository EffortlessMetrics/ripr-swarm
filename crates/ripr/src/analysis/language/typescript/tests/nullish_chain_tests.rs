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
    assert!(parameter_may_have_default(
        "function f(p = { a: 1 }) {}",
        "p"
    ));
    // An empty-object default leaves a property read nullish (#15357).
    assert!(!parameter_may_have_default("function f(p = {}) {}", "p"));
    assert!(!parameter_may_have_default(
        "function f(p: Opts = {}) {}",
        "p"
    ));
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

#[test]
fn proto_entry_in_the_argument_fails_open() -> Result<(), String> {
    let finding = finding(
        "nullish-proto-entry",
        "expect(displayName({ __proto__: { name: \"K\" } })).toBe(\"K\");",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn default_reader_skips_a_generic_list() {
    assert!(parameter_may_have_default(
        "function f<T = {}>(p: T = x) {}",
        "p"
    ));
    assert!(!parameter_may_have_default(
        "function f<T = {}>(p: T) {}",
        "p"
    ));
}

// #15357: a changed `const x = options.p ?? <literal>` default names the call
// that omits `p` when every observed call passes it.
const SLUG_OWNER: &str = "export function slugify(text: string, options: Opts = {}): string {\n  const lowercase = options.lowercase ?? true;\n  return lowercase ? text.toLowerCase() : text;\n}\n";
const SLUG_LINE: &str = "  const lowercase = options.lowercase ?? true;";
const SLUG_OLD: &str = "  const lowercase = options.lowercase ?? false;";

fn slug_finding(label: &str, owner: &str, line: &str, body: &str) -> Result<Finding, String> {
    let test = format!(
        "import {{ slugify }} from \"../src/settings\";\ntest(\"slug\", () => {{\n  {body}\n}});\n"
    );
    nested_callback_tests::settings_finding_in(label, owner, (2, line), &test, SLUG_OLD)
}

fn omission_facts(finding: &Finding) -> Vec<&str> {
    finding
        .activation
        .missing_discriminators
        .iter()
        .map(|fact| fact.value.as_str())
        .collect()
}

#[test]
fn changed_default_names_the_call_that_omits_the_option() -> Result<(), String> {
    let finding = slug_finding(
        "default-explicit",
        SLUG_OWNER,
        SLUG_LINE,
        "expect(slugify(\"Hello World\", { lowercase: true })).toBe(\"hello world\");\n  expect(slugify(\"a\", { lowercase: false })).toBe(\"a\");",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        omission_facts(&finding),
        vec!["call `slugify` without `lowercase` and assert the result of the `true` default"]
    );
    Ok(())
}

#[test]
fn changed_default_names_nothing_when_a_call_may_reach_it() -> Result<(), String> {
    for (label, owner, body) in [
        // A call that omits the option reaches the default.
        (
            "default-omitted",
            SLUG_OWNER,
            "expect(slugify(\"Hello World\")).toBe(\"hello world\");",
        ),
        // One call among several omits it.
        (
            "default-mixed",
            SLUG_OWNER,
            "expect(slugify(\"a\", { lowercase: true })).toBe(\"a\");\n  expect(slugify(\"B\")).toBe(\"b\");",
        ),
        // `undefined` is not shown to be a readable object.
        (
            "default-undefined",
            SLUG_OWNER,
            "expect(slugify(\"a\", undefined)).toBe(\"a\");",
        ),
        // The option object is not readable at the call.
        (
            "default-opaque",
            SLUG_OWNER,
            "const o = load();\n  expect(slugify(\"a\", o)).toBe(\"a\");",
        ),
        // A non-empty default object can supply the property.
        (
            "default-nonempty",
            "export function slugify(text: string, options: Opts = { lowercase: false }): string {\n  const lowercase = options.lowercase ?? true;\n  return lowercase ? text.toLowerCase() : text;\n}\n",
            "expect(slugify(\"a\", { lowercase: true })).toBe(\"a\");",
        ),
    ] {
        let finding = slug_finding(label, owner, SLUG_LINE, body)?;
        assert!(omission_facts(&finding).is_empty(), "{label}");
    }
    Ok(())
}

#[test]
fn changed_default_reader_is_closed() {
    let params = vec!["options".to_string()];
    let read = |old: &str, new: &str| NullishChainChange::read(old, new, &params);
    let default = |old: &str, new: &str| {
        read(old, new).and_then(|change| change.changed_default().map(|(p, v)| (p.to_string(), v)))
    };
    assert_eq!(
        default(SLUG_OLD, SLUG_LINE),
        Some(("lowercase".to_string(), "true".to_string()))
    );
    assert_eq!(
        default(
            "const sep = options.sep ?? \"-\";",
            "const sep = options.sep ?? \"_\";"
        ),
        Some(("sep".to_string(), "\"_\"".to_string()))
    );
    // The statement head must match on both sides (`return` vs `const`).
    assert!(read("return options.a ?? 1;", "const x = options.a ?? 2;").is_none());
    assert!(read("const x = options.a ?? 1;", "return options.a ?? 2;").is_none());
    // A renamed local, another keyword or a typed declaration is not read.
    assert!(read(SLUG_OLD, "  const lower = options.lowercase ?? true;").is_none());
    assert!(read(SLUG_OLD, "  var lowercase = options.lowercase ?? true;").is_none());
    assert!(
        read(
            "const x: boolean = options.a ?? false;",
            "const x: boolean = options.a ?? true;"
        )
        .is_none()
    );
    // A non-literal default names no value; a longer chain is not a default.
    assert!(default(SLUG_OLD, "  const lowercase = options.lowercase ?? other;").is_none());
    assert!(
        default(
            "const x = options.a ?? options.b ?? 1;",
            "const x = options.a ?? options.b ?? 2;"
        )
        .is_none()
    );
}

// #15357 review: with `p = {}`, an explicit `undefined` binds `{}`, whose
// inherited `constructor` the changed operand reads, so the call can tell the
// versions apart and the verdict stays put.
#[test]
fn empty_object_default_keeps_inherited_reads_for_undefined() -> Result<(), String> {
    const DEFAULT_OWNER: &str = "export function displayName(person = {}) {\n  return person?.nick ?? person?.constructor ?? \"x\";\n}\n";
    const DEFAULT_LINE: &str = "  return person?.nick ?? person?.constructor ?? \"x\";";
    const DEFAULT_OLD: &str = "  return person?.nick ?? person?.name ?? \"x\";";
    let finding_for = |label: &str, body: &str| -> Result<Finding, String> {
        let test = format!(
            "import {{ displayName }} from \"../src/settings\";\ntest(\"name\", () => {{\n  {body}\n}});\n"
        );
        nested_callback_tests::settings_finding_in(
            label,
            DEFAULT_OWNER,
            (2, DEFAULT_LINE),
            &test,
            DEFAULT_OLD,
        )
    };
    let inherited = finding_for(
        "default-undefined-inherited",
        "expect(displayName(undefined)).toBe(\"x\");",
    )?;
    assert_eq!(inherited.class, ExposureClass::Exposed);
    // Control: the same owner without a default reads both operands as
    // nullish for `undefined`, so the downgrade still applies.
    let control = nested_callback_tests::settings_finding_in(
        "default-undefined-control",
        &DEFAULT_OWNER.replace("person = {}", "person"),
        (2, DEFAULT_LINE),
        "import { displayName } from \"../src/settings\";\ntest(\"name\", () => {\n  expect(displayName(undefined)).toBe(\"x\");\n});\n",
        DEFAULT_OLD,
    )?;
    assert_eq!(control.class, ExposureClass::WeaklyExposed);
    assert!(parameter_has_empty_object_default(DEFAULT_OWNER, "person"));
    assert!(!parameter_has_empty_object_default(
        "function f(p = { a: 1 }) {}",
        "p"
    ));
    assert!(!parameter_has_empty_object_default("function f(p) {}", "p"));
    Ok(())
}
