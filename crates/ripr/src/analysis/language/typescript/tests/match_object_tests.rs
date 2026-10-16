//! RIPR-SPEC-0250 rule 1: `toMatchObject({ key: literal })` reads exact for
//! a changed field of the one object literal the owner returns.

use super::*;

const PROFILE: &str = "export function buildProfile(name: string, age: number) {\n  return {\n    name,\n    adult: age >= 18,\n    tags: [] as string[],\n  };\n}\n";

const FIELD_OLD: &str = "    adult: age >= 18,";
const FIELD_NEW: &str = "    adult: 18 <= age,";

#[test]
fn returned_field_key_reads_only_a_top_level_field_of_the_one_returned_literal() {
    assert_eq!(
        ts_owner_returned_field_key(PROFILE, FIELD_NEW).as_deref(),
        None,
        "the changed text must be in the owner"
    );
    assert_eq!(
        ts_owner_returned_field_key(PROFILE, FIELD_OLD).as_deref(),
        Some("adult")
    );
    // Without a trailing comma, and a string value with braces in it.
    let last = PROFILE.replace("    tags: [] as string[],\n", "    tags: \"{}\"\n");
    assert_eq!(
        ts_owner_returned_field_key(&last, "    tags: \"{}\"").as_deref(),
        Some("tags")
    );
    // `__proto__:` sets the prototype, not an own field (#8133).
    let proto = PROFILE.replace("    adult: age >= 18,\n", "    __proto__: base,\n");
    assert_eq!(
        ts_owner_returned_field_key(&proto, "    __proto__: base,").as_deref(),
        None
    );
    for (label, owner, line) in [
        (
            "a second return",
            PROFILE.replace(
                "  return {\n",
                "  if (age < 0) {\n    return null;\n  }\n  return {\n",
            ),
            FIELD_OLD,
        ),
        (
            "a nested literal",
            PROFILE.replace(
                "    adult: age >= 18,\n",
                "    meta: {\n      adult: age >= 18,\n    },\n",
            ),
            "      adult: age >= 18,",
        ),
        (
            "a spread can override the field",
            PROFILE.replace("    tags:", "    ...base,\n    tags:"),
            FIELD_OLD,
        ),
        (
            "a later entry sets the key again",
            PROFILE.replace("    tags:", "    adult: false,\n    tags:"),
            FIELD_OLD,
        ),
        (
            "a later quoted key",
            PROFILE.replace("    tags:", "    \"adult\": false,\n    tags:"),
            FIELD_OLD,
        ),
        (
            "a later computed key",
            PROFILE.replace("    tags:", "    [\"adult\"]: false,\n    tags:"),
            FIELD_OLD,
        ),
        (
            "a ternary arm continuing another entry",
            PROFILE.replace(
                "    adult: age >= 18,\n",
                "    adult,\n    status: admin ?\n      adult : 18 <= age,\n",
            ),
            "      adult : 18 <= age,",
        ),
        (
            "a later entry on the changed line holds the change",
            PROFILE.replace(FIELD_OLD, "    adult: age >= 18, senior: 18 <= age,"),
            "    adult: age >= 18, senior: 18 <= age,",
        ),
        (
            "a later entry after multi-byte text holds the change",
            PROFILE.replace(
                FIELD_OLD,
                "    adult: age >= 18, label: \"\u{e9}\u{2014}\u{fc}\", senior: 18 <= age,",
            ),
            "    adult: age >= 18, label: \"\u{e9}\u{2014}\u{fc}\", senior: 18 <= age,",
        ),
        (
            "a second entry on the changed line",
            PROFILE.replace(FIELD_OLD, "    adult: age >= 18, adult: true,"),
            "    adult: age >= 18, adult: true,",
        ),
        (
            "a spread on the changed line",
            PROFILE.replace(FIELD_OLD, "    adult: age >= 18, ...overrides,"),
            "    adult: age >= 18, ...overrides,",
        ),
        (
            "a duplicate after another entry on a later line",
            PROFILE.replace("    tags:", "    id: 1, adult: true,\n    tags:"),
            FIELD_OLD,
        ),
        (
            "an accessor",
            PROFILE.replace(
                "    tags:",
                "    get adult() {\n      return true;\n    },\n    tags:",
            ),
            FIELD_OLD,
        ),
        (
            "a member read after the literal",
            PROFILE.replace("  };\n", "  }.adult;\n"),
            FIELD_OLD,
        ),
        (
            "an assigned literal, then returned",
            PROFILE
                .replace("  return {\n", "  const profile = {\n")
                .replace("  };\n", "  };\n  return profile;\n"),
            FIELD_OLD,
        ),
        (
            "an arrow in the body",
            PROFILE.replace(
                "    tags: [] as string[],\n",
                "    tags: [].map((t) => t),\n",
            ),
            FIELD_OLD,
        ),
        (
            "a value that opens a literal",
            PROFILE.replace(
                "    adult: age >= 18,\n",
                "    adult: {\n      ok: age >= 18,\n    },\n",
            ),
            "    adult: {",
        ),
        (
            "not a field line",
            PROFILE.replace("  return {\n", "  const limit = 18;\n  return {\n"),
            "  const limit = 18;",
        ),
    ] {
        assert_eq!(ts_owner_returned_field_key(&owner, line), None, "{label}");
    }
}

fn profile_finding(label: &str, test: &str) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(
        &root.join("src/profile.ts"),
        &PROFILE.replace(FIELD_OLD, FIELD_NEW),
    )?;
    ts_write_file(&root.join("tests/profile.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/profile.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: FIELD_NEW.to_string(),
            new_side_line: 4,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: FIELD_OLD.to_string(),
            new_side_line: 4,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == 4)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn profile_test(body: &str) -> String {
    format!(
        "import {{ buildProfile }} from \"../src/profile\";\ntest(\"profile records adulthood\", async () => {{\n{body}\n}});\n"
    )
}

#[test]
fn match_object_pin_exposes_a_listed_returned_field() -> Result<(), String> {
    let pinned = profile_test(
        "  expect(buildProfile(\"ana\", 17)).toMatchObject({ name: \"ana\", adult: false });\n  expect(buildProfile(\"bo\", 18)).toMatchObject({ name: \"bo\", adult: true });",
    );
    let tests = extract_tests(Path::new("tests/profile.test.ts"), &pinned);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions.len(), 2);
    assert_eq!(tests[0].assertions[0].matcher, "toMatchObject");
    assert_eq!(
        tests[0].assertions[0].oracle_kind,
        OracleKind::RelationalCheck
    );
    assert_eq!(
        tests[0].assertions[0].expected_value_or_variant.as_deref(),
        Some("{ name: \"ana\", adult: false }")
    );
    let finding = profile_finding("match-object-pin", &pinned)?;
    assert_eq!(finding.probe.family, ProbeFamily::FieldConstruction);
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let row = finding
        .related_tests
        .first()
        .ok_or("expected a related test row")?;
    assert_eq!(row.oracle_kind, OracleKind::ExactValue);
    assert_eq!(row.oracle_strength, OracleStrength::Strong);

    for (label, body) in [
        (
            "match-object-local",
            "  const bo = buildProfile(\"bo\", 18);\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "match-object-quoted-listed-key",
            "  expect(buildProfile(\"ana\", 17)).toMatchObject({ \"adult\": false });",
        ),
    ] {
        let finding = profile_finding(label, &profile_test(body))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

#[test]
fn match_object_pin_stays_weak_without_the_listed_field_on_the_whole_result() -> Result<(), String>
{
    for (label, body) in [
        (
            "match-object-unlisted",
            "  expect(buildProfile(\"bo\", 18)).toMatchObject({ name: \"bo\" });",
        ),
        (
            "match-object-wrapped",
            "  expect(normalize(buildProfile(\"bo\", 18))).toMatchObject({ adult: true });",
        ),
        (
            "match-object-field-read",
            "  expect(buildProfile(\"bo\", 18).meta).toMatchObject({ adult: true });",
        ),
        (
            "match-object-dynamic",
            "  let expected = { adult: true };\n  expect(buildProfile(\"bo\", 18)).toMatchObject(expected);",
        ),
        (
            "match-object-rejects",
            "  await expect(buildProfile(\"bo\", 18)).rejects.toMatchObject({ adult: true });",
        ),
        (
            "match-object-shadowed",
            "  const buildProfile = (n: string, a: number) => ({ name: n, adult: a > 0 });\n  expect(buildProfile(\"bo\", 18)).toMatchObject({ adult: true });",
        ),
    ] {
        let finding = profile_finding(label, &profile_test(body))?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
        // The test must still relate, so a case that stopped parsing cannot
        // pass here; its row keeps a reading below `exact_value` / strong.
        if label != "match-object-shadowed" {
            let row = finding
                .related_tests
                .first()
                .ok_or_else(|| format!("{label}: expected a related test row"))?;
            assert_ne!(
                (row.oracle_kind.clone(), row.oracle_strength.clone()),
                (OracleKind::ExactValue, OracleStrength::Strong),
                "{label}"
            );
            assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
        }
    }
    Ok(())
}

#[test]
fn match_object_expected_with_escaped_proto_key_is_refused_as_pin() -> Result<(), String> {
    for (label, body) in [
        (
            "bare-proto",
            "  expect(buildProfile(\"bo\", 18)).toMatchObject({ __proto__: false });",
        ),
        (
            "quoted-proto",
            "  expect(buildProfile(\"bo\", 18)).toMatchObject({ \"__proto__\": false });",
        ),
        (
            "escaped-proto",
            "  expect(buildProfile(\"bo\", 18)).toMatchObject({ \"\\u005f_proto__\": false });",
        ),
    ] {
        let finding = profile_finding(label, &profile_test(body))?;
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: expected object with proto key must not be credited as pin: {:?}",
            finding.missing
        );
        let row = finding
            .related_tests
            .first()
            .ok_or_else(|| format!("{label}: expected a related test row"))?;
        assert_ne!(
            (row.oracle_kind.clone(), row.oracle_strength.clone()),
            (OracleKind::ExactValue, OracleStrength::Strong),
            "{label}"
        );
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    }
    Ok(())
}

/// The refused matcher readings keep their parsed subject: a dynamic
/// argument records no expected value and keeps the unresolved-assertion
/// limitation, and `.rejects.toMatchObject` keeps its RIPR-SPEC-0243
/// error-payload reading (`unknown` when the payload is not a literal), so
/// neither is a `relational_check` pin.
#[test]
fn match_object_refused_readings_keep_their_parsed_subject() -> Result<(), String> {
    for (label, body, kind, dynamic) in [
        (
            "dynamic",
            "  const expected = { adult: true };\n  expect(buildProfile(\"bo\", 18)).toMatchObject(expected);",
            OracleKind::RelationalCheck,
            true,
        ),
        (
            "rejects",
            "  await expect(buildProfile(\"bo\", 18)).rejects.toMatchObject({ adult: true });",
            OracleKind::ExactErrorVariant,
            false,
        ),
        (
            "rejects-dynamic",
            "  const expected = { adult: true };\n  await expect(buildProfile(\"bo\", 18)).rejects.toMatchObject(expected);",
            OracleKind::Unknown,
            true,
        ),
    ] {
        let tests = extract_tests(Path::new("test/profile.test.ts"), &profile_test(body));
        let [test] = tests.as_slice() else {
            return Err(format!("{label}: expected one test, got {}", tests.len()));
        };
        let assertion = test
            .assertions
            .iter()
            .find(|assertion| assertion.matcher == "toMatchObject")
            .ok_or_else(|| format!("{label}: no toMatchObject assertion"))?;
        assert_eq!(assertion.oracle_kind, kind, "{label}");
        assert_eq!(assertion.has_dynamic_matcher_arg, dynamic, "{label}");
        if dynamic {
            assert_eq!(assertion.expected_value_or_variant, None, "{label}");
        }
    }
    Ok(())
}

#[test]
fn match_object_facts_read_field_construction_changes_only() -> Result<(), String> {
    let root = ts_unique_tempdir("match-object-facts-family")?;
    ts_write_file(&root.join("src/profile.ts"), PROFILE)?;
    let test = profile_test(
        "  expect(buildProfile(\"bo\", 18)).toMatchObject({ adult: true });\n  expect(buildProfile(\"bo\", 18)).toMatchObject({ name: \"bo\" });",
    );
    ts_write_file(&root.join("tests/profile.test.ts"), &test)?;
    let owner = extract_owners(Path::new("src/profile.ts"), PROFILE)
        .into_iter()
        .find(|owner| owner.name == "buildProfile")
        .ok_or("owner buildProfile not extracted")?;
    let tests = extract_tests(Path::new("tests/profile.test.ts"), &test);
    let candidates =
        related_test_candidates(&owner, &tests, Some(&root), &ReExportIndex::empty(), None);
    let read = |family: ProbeFamily| {
        let facts =
            ts_match_object_facts(&family, FIELD_OLD, &owner, &candidates, None, Some(&root));
        tests[0]
            .assertions
            .iter()
            .filter(|assertion| facts.credits(&tests[0].file, assertion))
            .count()
    };
    let field = read(ProbeFamily::FieldConstruction);
    let predicate = read(ProbeFamily::Predicate);
    let _ = std::fs::remove_dir_all(&root);
    // Only the assertion that lists `adult` is credited.
    assert_eq!(field, 1);
    assert_eq!(predicate, 0);
    Ok(())
}

/// RIPR-SPEC-0250 example 4 (amended 2026-10-15): a pin on
/// `normalize(buildProfile(...))` reads as a pin on the owner call when
/// `normalize` is a test-local identity helper, and stays weak when the
/// helper rebuilds its argument.
#[test]
fn match_object_pin_looks_through_a_test_local_identity_helper() -> Result<(), String> {
    let pins = "test(\"profile records adulthood\", () => {\n  expect(normalize(buildProfile(\"ana\", 17))).toMatchObject({ name: \"ana\", adult: false });\n  expect(normalize(buildProfile(\"bo\", 18))).toMatchObject({ name: \"bo\", adult: true });\n});\n";
    let with_helper = |helper: &str| {
        format!("import {{ buildProfile }} from \"../src/profile\";\n{helper}\n{pins}")
    };
    let identity = with_helper("function normalize<T>(value: T): T {\n  return value;\n}");
    let tests = extract_tests(Path::new("tests/profile.test.ts"), &identity);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions.len(), 2);
    assert_eq!(
        tests[0].identity_helpers_in_file,
        vec!["normalize".to_string()]
    );
    let finding = profile_finding("match-object-identity", &identity)?;
    assert_eq!(finding.probe.family, ProbeFamily::FieldConstruction);
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let row = finding
        .related_tests
        .first()
        .ok_or("expected a related test row")?;
    assert_eq!(row.oracle_kind, OracleKind::ExactValue);
    assert_eq!(row.oracle_strength, OracleStrength::Strong);

    // Through the owner's namespace import the identity reading holds; a
    // body-local `ns` shadowing that import reaches the shadow instead.
    let namespaced = |body: &str| {
        format!(
            "import * as ns from \"../src/profile\";\nimport {{ buildProfile }} from \"../src/profile\";\nfunction normalize<T>(value: T): T {{\n  return value;\n}}\ntest(\"profile records adulthood\", () => {{\n{body}\n}});\n"
        )
    };
    let pin = "  expect(normalize(ns.buildProfile(\"bo\", 18))).toMatchObject({ name: \"bo\", adult: true });";
    let finding = profile_finding("match-object-namespace", &namespaced(pin))?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let shadowed = namespaced(&format!(
        "  const ns = fakeProfiles();\n  buildProfile(\"ana\", 17);\n{pin}"
    ));
    let tests = extract_tests(Path::new("tests/profile.test.ts"), &shadowed);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions.len(), 1);
    let finding = profile_finding("match-object-shadowed-namespace", &shadowed)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // A dynamic import is the owner namespace only as the whole
    // initializer: a chained `.then(...)` binds the callback's value.
    let dynamic = |init: &str| {
        format!(
            "import {{ buildProfile }} from \"../src/profile\";\nfunction normalize<T>(value: T): T {{\n  return value;\n}}\ntest(\"profile records adulthood\", async () => {{\n  const ns = {init};\n  buildProfile(\"ana\", 17);\n{pin}\n}});\n"
        )
    };
    let finding = profile_finding(
        "match-object-dynamic-namespace",
        &dynamic("await import(\"../src/profile\")"),
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let finding = profile_finding(
        "match-object-chained-dynamic-namespace",
        &dynamic("await import(\"../src/profile\").then(() => fakeProfiles())"),
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    // The helper stays eligible, so these refusals come from the call-site
    // shape: a member of the owner result, or a second wrap around it.
    for (label, shape) in [
        (
            "match-object-wrapped-member",
            "normalize(buildProfile(\"bo\", 18).meta)",
        ),
        (
            "match-object-nested-wrap",
            "normalize(normalize(buildProfile(\"bo\", 18)))",
        ),
    ] {
        let source = format!(
            "import {{ buildProfile }} from \"../src/profile\";\nfunction normalize<T>(value: T): T {{\n  return value;\n}}\ntest(\"profile records adulthood\", () => {{\n  buildProfile(\"ana\", 17);\n  expect({shape}).toMatchObject({{ name: \"bo\", adult: true }});\n}});\n"
        );
        let tests = extract_tests(Path::new("tests/profile.test.ts"), &source);
        assert_eq!(tests.len(), 1, "{label}");
        assert_eq!(tests[0].assertions.len(), 1, "{label}");
        assert_eq!(
            tests[0].identity_helpers_in_file,
            vec!["normalize".to_string()],
            "{label}"
        );
        let finding = profile_finding(label, &source)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{label}");
    }

    for (label, helper) in [
        (
            "match-object-rebuilding-helper",
            "function normalize<T>(value: T): T {\n  return { ...value, adult: true };\n}",
        ),
        (
            "match-object-reassigned-helper",
            "let normalize = <T>(value: T): T => value;",
        ),
    ] {
        let finding = profile_finding(label, &with_helper(helper))?;
        let row = finding
            .related_tests
            .first()
            .ok_or_else(|| format!("{label}: expected a related test row"))?;
        assert_ne!(
            (row.oracle_kind.clone(), row.oracle_strength.clone()),
            (OracleKind::ExactValue, OracleStrength::Strong),
            "{label}"
        );
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    }
    Ok(())
}

/// #7497: a local bound once to the owner result that the test rewrites
/// before asserting observes the test's own write, not the owner call, so
/// the whole-result credit stays unavailable. Every shape below read
/// `exposed` before the fix (false exposed); all must downgrade now — for
/// the `toMatchObject` pin and for the `toEqual` whole-object reading,
/// which shares the whole-result reader. The later review round widened
/// the guard to keyword-rooted member calls (`this.fix(bo)`), computed and
/// parenthesized callee escapes (`Object["assign"](bo, ...)`,
/// `(fix)(bo)`), generic type arguments (`fix<any>(bo)`), and non-null
/// bracket writes (`bo!["adult"] = true`); those pins keep each widened
/// branch honest.
#[test]
fn rewritten_owner_local_is_not_the_owner_result() -> Result<(), String> {
    for (label, body) in [
        (
            "rewritten-member-assign",
            "  const bo = buildProfile(\"bo\", 18);\n  bo.adult = true;\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-bracket-assign",
            "  const bo = buildProfile(\"bo\", 18);\n  bo[\"adult\"] = true;\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-object-assign",
            "  const bo = buildProfile(\"bo\", 18);\n  Object.assign(bo, { adult: true });\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-helper-escape",
            "  function fix(p: any) { p.adult = true; }\n  const bo = buildProfile(\"bo\", 18);\n  fix(bo);\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-member-assign-to-equal",
            "  const bo = buildProfile(\"bo\", 18);\n  bo.adult = true;\n  expect(bo).toEqual({ name: \"bo\", adult: true, tags: [] });",
        ),
        (
            "rewritten-object-assign-to-equal",
            "  const bo = buildProfile(\"bo\", 18);\n  Object.assign(bo, { adult: true });\n  expect(bo).toEqual({ name: \"bo\", adult: true, tags: [] });",
        ),
        (
            "rewritten-helper-escape-to-equal",
            "  function fix(p: any) { p.adult = true; }\n  const bo = buildProfile(\"bo\", 18);\n  fix(bo);\n  expect(bo).toEqual({ name: \"bo\", adult: true, tags: [] });",
        ),
        (
            "rewritten-this-escape",
            "  const bo = buildProfile(\"bo\", 18);\n  this.fix(bo);\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-computed-object-assign",
            "  const bo = buildProfile(\"bo\", 18);\n  Object[\"assign\"](bo, { adult: true });\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-computed-object-assign-to-equal",
            "  const bo = buildProfile(\"bo\", 18);\n  Object[\"assign\"](bo, { adult: true });\n  expect(bo).toEqual({ name: \"bo\", adult: true, tags: [] });",
        ),
        (
            "rewritten-parenthesized-escape",
            "  const bo = buildProfile(\"bo\", 18);\n  (fix)(bo);\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-generic-escape",
            "  const bo = buildProfile(\"bo\", 18);\n  fix<any>(bo);\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-nonnull-bracket-assign",
            "  const bo = buildProfile(\"bo\", 18);\n  bo![\"adult\"] = true;\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-nonnull-bracket-assign-to-equal",
            "  const bo = buildProfile(\"bo\", 18);\n  bo![\"adult\"] = true;\n  expect(bo).toEqual({ name: \"bo\", adult: true, tags: [] });",
        ),
        (
            "rebound-bare-let",
            "  let bo = buildProfile(\"bo\", 18);\n  bo = { name: \"bo\", adult: true, tags: [] };\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            // No seeding `bo.count = 0;` line: a plain member assign would
            // trip the guard before the compound branch is reached, and the
            // case would pass even with `+=` detection deleted.
            "rewritten-compound-assign",
            "  const bo = buildProfile(\"bo\", 18);\n  bo.count += 1;\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "rewritten-delete-field",
            "  const bo = buildProfile(\"bo\", 18);\n  delete bo.adult;\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            // Seeding assignment removed for the same reason: this case
            // must exercise the postfix `++` branch itself.
            "rewritten-postfix-incr",
            "  const bo = buildProfile(\"bo\", 18);\n  bo.count++;\n  expect(bo).toMatchObject({ adult: true });",
        ),
    ] {
        let finding = profile_finding(label, &profile_test(body))?;
        assert_eq!(
            finding.class,
            ExposureClass::WeaklyExposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    // Preservation: an unmutated local still witnesses through the same
    // reader, for both the pin and the whole-object equality.
    for (label, body) in [
        (
            "unmutated-local-match-object",
            "  const bo = buildProfile(\"bo\", 18);\n  expect(bo).toMatchObject({ adult: true });",
        ),
        (
            "unmutated-local-to-equal",
            "  const bo = buildProfile(\"bo\", 18);\n  expect(bo).toEqual({ name: \"bo\", adult: true, tags: [] });",
        ),
    ] {
        let finding = profile_finding(label, &profile_test(body))?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    // Preservation through non-`expect` assertion spellings: a pristine
    // local asserted via `node:assert` or `chai.expect` must not be
    // downgraded by the mutation guard.
    for (label, source) in [
        (
            "unmutated-local-node-assert",
            "import { buildProfile } from \"../src/profile\";\nimport assert from \"node:assert\";\ntest(\"profile records adulthood\", async () => {\n  const bo = buildProfile(\"bo\", 18);\n  assert.deepStrictEqual(bo, { name: \"bo\", adult: true, tags: [] });\n});\n",
        ),
        (
            "unmutated-local-chai-expect",
            "import { buildProfile } from \"../src/profile\";\nimport * as chai from \"chai\";\ntest(\"profile records adulthood\", async () => {\n  const bo = buildProfile(\"bo\", 18);\n  chai.expect(bo).to.deep.equal({ name: \"bo\", adult: true, tags: [] });\n});\n",
        ),
    ] {
        let finding = profile_finding(label, source)?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    // Preservation through keyword-rooted reads: `if (bo)` keeps the
    // whole-result credit. The keyword exemption still applies while the
    // keyword is the whole callee (`root == last`), so only member calls
    // rooted in one (`this.fix(bo)`) reach the escape check.
    {
        let finding = profile_finding(
            "keyword-condition-read",
            &profile_test(
                "  const bo = buildProfile(\"bo\", 18);\n  if (bo) {\n    console.log(\"kept\");\n  }\n  expect(bo).toMatchObject({ adult: true });",
            ),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "keyword-condition-read: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// RIPR-SPEC-0250 example 5 (amended 2026-10-15): `toMatchObject(<name>)`
/// reads the flat literal of a test-body `const` the body names only in its
/// binding and that argument; any other mention, binding kind, object shape,
/// matcher chain or escape hatch keeps the pin uncredited.
#[test]
fn match_object_pin_reads_a_test_body_object_const() -> Result<(), String> {
    let pinned = profile_test(
        "  const minor = { name: \"ana\", adult: false };\n  const grown = { name: \"bo\", adult: true };\n  expect(buildProfile(\"ana\", 17)).toMatchObject(minor);\n  expect(buildProfile(\"bo\", 18)).toMatchObject(grown);",
    );
    let tests = extract_tests(Path::new("tests/profile.test.ts"), &pinned);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions.len(), 2);
    let assertion = &tests[0].assertions[1];
    assert_eq!(assertion.matcher, "toMatchObject");
    assert_eq!(assertion.expected_value_or_variant, None);
    assert!(assertion.has_dynamic_matcher_arg);
    assert_eq!(
        assertion.expected_object_const.as_deref(),
        Some("{ name: \"bo\", adult: true }")
    );
    let finding = profile_finding("match-object-const", &pinned)?;
    assert_eq!(finding.probe.family, ProbeFamily::FieldConstruction);
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    let row = finding
        .related_tests
        .first()
        .ok_or("expected a related test row")?;
    assert_eq!(row.oracle_kind, OracleKind::ExactValue);
    assert_eq!(row.oracle_strength, OracleStrength::Strong);
    // The argument is resolved, so the finding does not also say it is not.
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("typescript_dynamic_assertion_unresolved")),
        "{:?}",
        finding.evidence
    );

    let pin = "  expect(buildProfile(\"bo\", 18)).toMatchObject(expected);";
    let declared = "  const expected = { adult: true };";
    for (label, body) in [
        ("let", format!("  let expected = {{ adult: true }};\n{pin}")),
        (
            "field-write",
            format!("{declared}\n  expected.adult = false;\n{pin}"),
        ),
        (
            "alias",
            format!(
                "{declared}\n  const same = expected;\n  expect(buildProfile(\"bo\", 18)).toMatchObject(same);"
            ),
        ),
        (
            "helper-call",
            format!("{declared}\n  tweak(expected);\n{pin}"),
        ),
        ("second-assertion", format!("{declared}\n{pin}\n{pin}")),
        (
            "unlisted-field",
            format!("  const expected = {{ name: \"bo\" }};\n{pin}"),
        ),
        (
            "nested-value",
            format!("  const expected = {{ adult: true, meta: {{ id: 1 }} }};\n{pin}"),
        ),
        (
            "spread",
            format!("  const expected = {{ ...base, adult: true }};\n{pin}"),
        ),
        ("eval", format!("{declared}\n  eval(\"x\");\n{pin}")),
        (
            "unicode-escape",
            format!("{declared}\n  const s = \"\\u0041\";\n{pin}"),
        ),
        ("nested-block", format!("  {{\n  {declared}\n  {pin}\n  }}")),
        // The binding is top-level but the call sits in a nested block.
        (
            "call-in-block",
            format!("{declared}\n  if (true) {{\n  {pin}\n  }}"),
        ),
        (
            "resolves-chain",
            format!(
                "{declared}\n  await expect(Promise.resolve(buildProfile(\"bo\", 18))).resolves.toMatchObject(expected);"
            ),
        ),
    ] {
        let test = profile_test(&body);
        let tests = extract_tests(Path::new("tests/profile.test.ts"), &test);
        assert_eq!(tests.len(), 1, "{label}");
        let assertion = tests[0]
            .assertions
            .iter()
            .find(|assertion| assertion.matcher == "toMatchObject")
            .ok_or_else(|| format!("{label}: no toMatchObject assertion"))?;
        if label != "unlisted-field" {
            assert_eq!(assertion.expected_object_const, None, "{label}");
        }
        let finding = profile_finding(&format!("match-object-const-{label}"), &test)?;
        let row = finding
            .related_tests
            .first()
            .ok_or_else(|| format!("{label}: expected a related test row"))?;
        assert_ne!(
            (row.oracle_kind.clone(), row.oracle_strength.clone()),
            (OracleKind::ExactValue, OracleStrength::Strong),
            "{label}"
        );
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    }

    // A binding outside the test body is shared with other tests and hooks,
    // so it is not read even when this body names it only once.
    let outside = format!(
        "import {{ buildProfile }} from \"../src/profile\";\n{declared}\ntest(\"profile records adulthood\", () => {{\n{pin}\n}});\n"
    );
    let tests = extract_tests(Path::new("tests/profile.test.ts"), &outside);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions.len(), 1);
    assert_eq!(tests[0].assertions[0].expected_object_const, None);
    let finding = profile_finding("match-object-const-outside-body", &outside)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);

    // A sloppy-mode script can reach the object through a `with` scope.
    let script = |extra: &str| {
        let source = format!(
            "const {{ buildProfile }} = require(\"../src/profile\");\ntest(\"t\", () => {{\n{declared}\n{extra}{pin}\n}});\n"
        );
        let tests = extract_tests(Path::new("tests/profile.test.cjs"), &source);
        assert_eq!(tests.len(), 1, "{extra}");
        tests[0]
            .assertions
            .iter()
            .find(|assertion| assertion.matcher == "toMatchObject")
            .and_then(|assertion| assertion.expected_object_const.clone())
    };
    assert_eq!(script("").as_deref(), Some("{ adult: true }"));
    assert_eq!(script("  with (globalThis) {\n  }\n"), None);
    Ok(())
}

/// `{ __proto__: v }` sets the object's prototype rather than an own
/// property, so a colon `__proto__:` entry is not a listed field on either
/// the matcher argument or the owner's returned literal.
///
/// Computed `["__proto__"]` and shorthand `__proto__` are own properties.
/// On current main they already fail this reader's colon/`[]` split, so
/// they are not cases here; the skip must not treat them as proto setters.
#[test]
fn colon_proto_is_not_a_listed_match_object_field() -> Result<(), String> {
    let owner =
        "export function make() {\n  return {\n    name: \"x\",\n    __proto__: 0,\n  };\n}\n";
    assert_eq!(
        ts_owner_returned_field_key(owner, "    __proto__: 0,").as_deref(),
        None,
        "owner colon __proto__: is a prototype setter, not a returned field"
    );

    for text in [
        "{ __proto__: 0 }",
        "{ \"__proto__\": 0 }",
        "{ '__proto__': 0 }",
    ] {
        let keys = flat_object_keys(text);
        assert!(
            keys.as_ref()
                .is_none_or(|keys| !keys.iter().any(|key| key == "__proto__")),
            "{text}: colon __proto__ must not list an own field, got {keys:?}"
        );
    }
    assert_eq!(
        flat_object_keys("{ __proto__: 0, adult: true }"),
        Some(vec!["adult".to_string()]),
        "skipping colon __proto__: must still list sibling own fields"
    );

    let root = ts_unique_tempdir("match-object-colon-proto")?;
    ts_write_file(
        &root.join("src/make.ts"),
        &owner.replace("    __proto__: 0,", "    __proto__: 1,"),
    )?;
    ts_write_file(
        &root.join("tests/make.test.ts"),
        "import { make } from \"../src/make\";\ntest(\"proto\", () => {\n  expect(make()).toMatchObject({ __proto__: 1 });\n});\n",
    )?;
    let changed = ChangedFile {
        path: PathBuf::from("src/make.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: "    __proto__: 1,".to_string(),
            new_side_line: 4,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line: 4,
            text: "    __proto__: 0,".to_string(),
            new_side_line: 4,
        }],
    };
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed],
    );
    let _ = std::fs::remove_dir_all(&root);
    let finding = result?
        .findings
        .into_iter()
        .find(|finding| finding.probe.location.line == 4)
        .ok_or("expected a finding on the changed __proto__: line")?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "colon __proto__: pin would credit a prototype setter: {:?}",
        finding.missing
    );
    let row = finding
        .related_tests
        .first()
        .ok_or("expected a related test row")?;
    assert_ne!(
        (row.oracle_kind.clone(), row.oracle_strength.clone()),
        (OracleKind::ExactValue, OracleStrength::Strong)
    );

    // A listed own field still pins when the matcher also has a colon
    // proto setter; computed/shorthand siblings already fail this parse.
    let finding = profile_finding(
        "colon-sibling-adult",
        &profile_test(
            "  expect(buildProfile(\"bo\", 18)).toMatchObject({ __proto__: 0, adult: true });",
        ),
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "colon-sibling-adult: {:?}",
        finding.missing
    );
    Ok(())
}
