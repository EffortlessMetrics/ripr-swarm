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
            "match-object-resolves",
            "  await expect(Promise.resolve(buildProfile(\"bo\", 18))).resolves.toMatchObject({ adult: true });\n  expect(buildProfile(\"ana\", 17)).toMatchObject({ \"adult\": false });",
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
            "  const expected = { adult: true };\n  expect(buildProfile(\"bo\", 18)).toMatchObject(expected);",
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
