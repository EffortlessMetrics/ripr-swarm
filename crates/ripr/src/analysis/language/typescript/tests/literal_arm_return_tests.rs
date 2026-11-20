//! #15306: a changed literal return of one arm of a literal-return lookup.

use super::*;

fn owner(source: &str) -> TypeScriptOwner {
    TypeScriptOwner {
        name: "dir".to_string(),
        file: PathBuf::from("src/dir.ts"),
        start_line: 10,
        end_line: 10 + source.lines().count(),
        source_text: Some(source.to_string()),
        ..test_owner("dir", "src/dir.ts")
    }
}

const SWITCH: &str = "export function dir(n: number): string {\n  switch (n) {\n    case 1:\n      return 'uno';\n    case 2:\n      return 'two';\n    default:\n      return 'other';\n  }\n}\n";

#[test]
fn a_switch_arm_literal_return_is_located_with_its_old_value() {
    let change = literal_arm_return(
        &owner(SWITCH),
        13,
        "      return 'uno';",
        Some("      return 'one';"),
    );
    assert_eq!(
        change,
        Some(LiteralArmReturn {
            old_value: ArmConstant::Text("one".to_string()),
            old_text: "'one'".to_string(),
            new_value: ArmConstant::Text("uno".to_string()),
            new_value_shared: false,
            line: 13,
            line_text: "      return 'uno';".to_string(),
        })
    );
}

#[test]
fn an_old_literal_another_arm_also_returns_is_refused() {
    let source = SWITCH.replace("'other'", "'one'");
    assert_eq!(
        literal_arm_return(
            &owner(&source),
            13,
            "      return 'uno';",
            Some("      return 'one';")
        ),
        None
    );
}

#[test]
fn a_non_literal_return_anywhere_in_the_owner_is_refused() {
    let source = SWITCH.replace("return 'other';", "return String(n);");
    assert_eq!(
        literal_arm_return(
            &owner(&source),
            13,
            "      return 'uno';",
            Some("      return 'one';")
        ),
        None
    );
}

#[test]
fn a_single_return_owner_and_a_line_mismatch_are_refused() {
    let single = "export function dir(n: number): string {\n  return 'uno';\n}\n";
    assert_eq!(
        literal_arm_return(
            &owner(single),
            11,
            "  return 'uno';",
            Some("  return 'one';")
        ),
        None
    );
    // The changed line must be the return the parse finds there.
    assert_eq!(
        literal_arm_return(
            &owner(SWITCH),
            12,
            "      return 'uno';",
            Some("      return 'one';")
        ),
        None
    );
}

#[test]
fn nested_function_returns_are_not_the_owners_and_semicolons_are_optional() {
    let source = "export const dir = (n: number) => {\n  const f = () => compute(n)\n  if (n > 0) return 'pos'\n  if (n < 0) return 'neg'\n  return 'zero'\n}\n";
    let change = literal_arm_return(
        &owner(source),
        13,
        "  if (n < 0) return 'neg'",
        Some("  if (n < 0) return 'negative'"),
    );
    // `if (...) return` on one line is not a plain return line.
    assert_eq!(change, None);
    let change = literal_arm_return(&owner(source), 14, "  return 'zero'", Some("  return 'z'"));
    assert_eq!(
        change.map(|change| change.old_value),
        Some(ArmConstant::Text("z".to_string()))
    );
}

#[test]
fn a_generator_or_concise_arrow_owner_is_refused() {
    let generator =
        "export function* dir(n: number) {\n  if (n) {\n    return 1;\n  }\n  return 2;\n}\n";
    assert_eq!(
        literal_arm_return(
            &owner(generator),
            12,
            "    return 1;",
            Some("    return 3;")
        ),
        None
    );
    let concise = "export const dir = (n: number) => n ? 1 : 2;\n";
    assert_eq!(
        literal_arm_return(&owner(concise), 10, "return 3;", Some("return 1;")),
        None
    );
}

#[test]
fn a_source_that_starts_after_the_owner_line_or_binds_undefined_is_refused() {
    // An owner span that disagrees with its recorded text (one line longer)
    // would offset into another line: the changed text is not there.
    let mut shifted = owner(SWITCH);
    shifted.end_line += 1;
    assert_eq!(
        literal_arm_return(
            &shifted,
            13,
            "      return 'uno';",
            Some("      return 'one';")
        ),
        None
    );
    // A parameter named `undefined` makes `return undefined` its value.
    let bound = "export function dir(n: number, undefined: number): unknown {\n  if (n === 1) {\n    return undefined;\n  }\n  return 2;\n}\n";
    assert_eq!(
        literal_arm_return(
            &owner(bound),
            12,
            "    return undefined;",
            Some("    return 1;")
        ),
        None
    );
}

#[test]
fn an_undefined_return_is_refused_without_a_proven_binding() {
    // A module-level binding and a comment between `const` and the name both
    // shadow the global; the recorded source cannot prove which one a
    // `return undefined` reads, so the rule declines instead of treating the
    // identifier as a scalar.
    for source in [
        "const undefined = 1;\nexport function dir(n: number): unknown {\n  if (n === 1) {\n    return undefined;\n  }\n  return 2;\n}\n",
        "const /* shadow */ undefined = 1;\nexport function dir(n: number): unknown {\n  if (n === 1) {\n    return undefined;\n  }\n  return 2;\n}\n",
    ] {
        assert_eq!(
            literal_arm_return(
                &owner(source),
                13,
                "    return undefined;",
                Some("    return 1;")
            ),
            None
        );
    }
    // Numeric control: the same shape with numbers still locates the arm.
    let numeric = "export function dir(n: number): number {\n  if (n === 1) {\n    return 7;\n  }\n  return 2;\n}\n";
    assert!(
        literal_arm_return(&owner(numeric), 12, "    return 7;", Some("    return 5;")).is_some()
    );
}

#[test]
fn a_bigint_return_is_not_a_readable_scalar() {
    // `5n` and `5` are different values: an arm returning `5n` neither
    // duplicates nor shares a changed `5`, so the reader refuses it.
    let source = "export function dir(n: number): unknown {\n  if (n === 1) {\n    return 7;\n  }\n  return 5n;\n}\n";
    assert_eq!(
        literal_arm_return(&owner(source), 12, "    return 7;", Some("    return 5;")),
        None
    );
    assert_eq!(
        literal_arm_return(
            &owner(&SWITCH.replace("'uno'", "1n")),
            13,
            "      return 1n;",
            Some("      return 'one';")
        ),
        None
    );
}

#[test]
fn an_arrow_owner_whose_text_starts_on_a_later_line_locates_the_return() {
    // `export const dir =` is line 10; the arrow's own text starts on line 11,
    // so the changed line must be offset from the text's origin, not from the
    // declaration's first line.
    let source = "(n: number): string => {\n  switch (n) {\n    case 1:\n      return 'uno';\n    case 2:\n      return 'two';\n    default:\n      return 'other';\n  }\n}";
    let mut owner = owner(source);
    owner.start_line = 10;
    owner.end_line = 20;
    let change = literal_arm_return(
        &owner,
        14,
        "      return 'uno';",
        Some("      return 'one';"),
    );
    assert!(
        change.is_some(),
        "the return on the arrow text's fourth line must be located"
    );
}

#[test]
fn a_class_method_owner_parses_inside_a_class_wrapper() {
    let method = "dir(n: number): number {\n    if (n === 1) {\n      return 10;\n    }\n    return 20;\n  }\n";
    let change = literal_arm_return(
        &owner(method),
        12,
        "      return 10;",
        Some("      return 11;"),
    );
    assert_eq!(
        change.map(|change| change.old_value),
        Some(ArmConstant::Number(11.0))
    );
}

const DIR: &str = "export function dir(n: number): string {\n  switch (n) {\n    case 1:\n      return 'one';\n    case 2:\n      return 'two';\n    default:\n      return 'other';\n  }\n}\n";

/// Runs the TypeScript adapter on one changed line of `src/dir.ts` with one
/// test file, returning the finding on that line.
fn finding_at(
    label: &str,
    source: &str,
    line: usize,
    removed: &str,
    added: &str,
    test: &str,
) -> Result<Finding, String> {
    let root = ts_unique_tempdir(label)?;
    let changed_source = source.replacen(removed.trim(), added.trim(), 1);
    ts_write_file(&root.join("src/dir.ts"), &changed_source)?;
    ts_write_file(&root.join("tests/dir.test.ts"), test)?;
    let changed = ChangedFile {
        path: PathBuf::from("src/dir.ts"),
        added_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: added.to_string(),
            new_side_line: line,
        }],
        removed_lines: vec![crate::analysis::diff::ChangedLine {
            line,
            text: removed.to_string(),
            new_side_line: line,
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
        .find(|finding| finding.probe.location.line == line)
        .ok_or_else(|| format!("{label}: expected a finding on the changed line"))
}

fn vitest(body: &str) -> String {
    format!(
        "import {{ describe, it, expect }} from 'vitest';\nimport {{ dir }} from '../src/dir';\n\ndescribe('dir', () => {{\n  it('maps', () => {{\n{body}  }});\n}});\n"
    )
}

/// geolib `getCompassDirection`: the only exact assertion expects another
/// arm's result, so changing the `case 1` arm passes the suite.
#[test]
fn an_arm_no_assertion_expects_is_not_exposed_and_names_the_arm() -> Result<(), String> {
    for (label, line, removed, added) in [
        ("arm-case", 4, "      return 'one';", "      return 'uno';"),
        (
            "arm-default",
            8,
            "      return 'other';",
            "      return 'o';",
        ),
    ] {
        let finding = finding_at(
            label,
            DIR,
            line,
            removed,
            added,
            &vitest("    expect(dir(2)).toBe('two');\n"),
        )?;
        assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{label}");
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains("expect other arms' results (`'two'`)")),
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// The discriminating positive: the assertion that expects the arm's old
/// result keeps `exposed`, written with either quote.
#[test]
fn an_assertion_expecting_the_arms_old_result_keeps_exposed() -> Result<(), String> {
    for (label, body) in [
        ("arm-pinned", "    expect(dir(1)).toBe('one');\n"),
        (
            "arm-pinned-double",
            "    expect(dir(2)).toBe('two');\n    expect(dir(1)).toEqual(\"one\");\n",
        ),
        // A derived local still names the old value: kept, never ruled out.
        (
            "arm-pinned-local",
            "    const label = dir(1);\n    expect(label).toBe('one');\n",
        ),
        // The old result inside a wrapping object or array is still pinned.
        (
            "arm-pinned-object",
            "    expect(dir(2)).toBe('two');\n    expect({ d: dir(1) }).toEqual({ d: 'one' });\n",
        ),
        (
            "arm-pinned-array",
            "    expect(dir(2)).toBe('two');\n    expect([dir(1), dir(2)]).toEqual(['one', 'two']);\n",
        ),
    ] {
        let finding = finding_at(
            label,
            DIR,
            4,
            "      return 'one';",
            "      return 'uno';",
            &vitest(body),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// An exact assertion on the selected arm that expects its new result also
/// tells the versions apart (#12991 `switch_literal_return_controls`): the
/// base returns the old literal there. An assertion expecting an unchanged
/// arm's distinct result still removes the credit.
#[test]
fn an_assertion_expecting_the_arms_new_result_keeps_exposed() -> Result<(), String> {
    for (label, body) in [
        ("arm-new-direct", "    expect(dir(1)).toBe('uno');\n"),
        (
            "arm-new-with-other",
            "    expect(dir(2)).toBe('two');\n    expect(dir(1)).toEqual(\"uno\");\n",
        ),
        (
            "arm-new-object",
            "    expect(dir(2)).toBe('two');\n    expect({ d: dir(1) }).toEqual({ d: 'uno' });\n",
        ),
    ] {
        let finding = finding_at(
            label,
            DIR,
            4,
            "      return 'one';",
            "      return 'uno';",
            &vitest(body),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    let finding = finding_at(
        "arm-new-absent",
        DIR,
        4,
        "      return 'one';",
        "      return 'uno';",
        &vitest("    expect(dir(2)).toBe('two');\n    expect(dir(3)).toBe('other');\n"),
    )?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    Ok(())
}

/// The paired truth table when the new literal is shared: `gold` stays 7
/// while `silver` moves 5 -> 7. `silver` expecting 5 passes on the base
/// only and `silver` expecting 7 on the candidate only, so both keep
/// `exposed`; `gold` expecting 7 passes on both, so its credit is removed
/// because the call provably leaves the changed arm closed. A selector the
/// reader cannot evaluate keeps the verdict.
#[test]
fn a_shared_new_result_is_credited_only_on_the_selected_arm() -> Result<(), String> {
    const PRICE: &str = "export function dir(kind: string): number {\n  switch (kind) {\n    case 'gold':\n      return 7;\n    case 'silver':\n      return 5;\n    default:\n      return 0;\n  }\n}\n";
    let run = |label: &str, body: &str| {
        finding_at(
            label,
            PRICE,
            6,
            "      return 5;",
            "      return 7;",
            &vitest(body),
        )
    };
    for (label, body) in [
        ("shared-silver-old", "    expect(dir('silver')).toBe(5);\n"),
        ("shared-silver-new", "    expect(dir('silver')).toBe(7);\n"),
        (
            "shared-silver-local",
            "    const result = dir('silver');\n    expect(result).toBe(7);\n",
        ),
        ("shared-computed", "    expect(dir(pick())).toBe(7);\n"),
        ("shared-spread", "    expect(dir(...['gold'])).toBe(7);\n"),
        (
            "shared-double-quote",
            "    expect(dir(\"silver\")).toBe(7);\n",
        ),
        (
            "shared-gold-and-silver",
            "    expect(dir('gold')).toBe(7);\n    expect(dir('silver')).toBe(7);\n",
        ),
        // A comma inside a block or line comment is not an argument slot: the
        // argument list stays unreadable, so the shared-result exclusion does
        // not apply and the verdict is kept.
        (
            "shared-comment-comma",
            "    expect(dir('gold' /* , 'silver' */)).toBe(7);\n",
        ),
        (
            "shared-line-comment-comma",
            "    expect(dir('gold' // , 'silver'\n    )).toBe(7);\n",
        ),
    ] {
        let finding = run(label, body)?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    for (label, body) in [
        ("shared-gold", "    expect(dir('gold')).toBe(7);\n"),
        (
            "shared-gold-local",
            "    const result = dir('gold');\n    expect(result).toBe(7);\n",
        ),
        (
            "shared-gold-and-default",
            "    expect(dir('gold')).toBe(7);\n    expect(dir('bronze')).toBe(0);\n",
        ),
    ] {
        let finding = run(label, body)?;
        assert_eq!(
            finding.class,
            ExposureClass::InfectionUnknown,
            "{label}: {:?}",
            finding.missing
        );
        // The literal-arm rule itself removed the credit, naming the
        // unchanged arm's result the assertion expects.
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains("expect other arms' results (") && line.contains("`7`")),
            "{label}: {:?}",
            finding.missing
        );
    }
    // A comment inside the arguments keeps the verdict.
    let finding = run(
        "shared-comment-arguments",
        "    expect(dir('silver' /* , 'gold' */)).toBe(7);\n",
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    // A changed `default` arm reads no selecting condition, so a call to
    // another arm expecting the shared new literal keeps the verdict.
    let finding = finding_at(
        "shared-default-arm",
        PRICE,
        8,
        "      return 0;",
        "      return 7;",
        &vitest("    expect(dir('gold')).toBe(7);\n"),
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// A recursive owner can re-enter the changed arm from inside a call that
/// skips it: `dir('gold')` reads `dir('silver')`, so `gold` expecting the
/// shared new result 7 tells the versions apart (1 on the base).
#[test]
fn a_recursive_owner_keeps_a_shared_new_result() -> Result<(), String> {
    const SOURCE: &str = "export function dir(kind: string): number {\n  if (kind === 'gold') {\n    if (dir('silver') === 7) {\n      return 7;\n    }\n    return 1;\n  }\n  if (kind === 'silver') {\n    return 5;\n  }\n  return 0;\n}\n";
    let finding = finding_at(
        "shared-recursive",
        SOURCE,
        9,
        "    return 5;",
        "    return 7;",
        &vitest("    expect(dir('gold')).toBe(7);\n"),
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// A recursive owner can re-enter the changed arm with another input, so a
/// call that leaves the arm closed does not prove the shared new result came
/// from the unchanged arm: the verdict is kept.
#[test]
fn a_reentrant_owner_keeps_the_verdict_for_a_shared_new_result() -> Result<(), String> {
    const REENTRANT: &str = "export function dir(kind: string): number {\n  if (kind === 'again') {\n    dir('silver');\n  }\n  switch (kind) {\n    case 'gold':\n      return 7;\n    case 'silver':\n      return 5;\n    default:\n      return 0;\n  }\n}\n";
    let finding = finding_at(
        "shared-reentrant",
        REENTRANT,
        8,
        "      return 5;",
        "      return 7;",
        &vitest("    expect(dir('gold')).toBe(7);\n"),
    )?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "{:?}",
        finding.missing
    );
    Ok(())
}

/// An arrow owner whose arrow starts on the line after `export const dir =`
/// records its source from the arrow, one line after the owner's first
/// line. The changed return has a neighboring return on the next line, so
/// an off-by-one read would land on `return 'two'` and refuse the rule; the
/// rule instead locates the changed return and reads the same as the owner
/// written on one line.
#[test]
fn an_arrow_on_the_next_line_locates_the_changed_return() -> Result<(), String> {
    const BODY: &str = "    if (n === 1)\n      return 'one';\n    return 'two';\n  };\n";
    for (label, source, line) in [
        (
            "arm-arrow-next-line",
            format!("export const dir =\n  (n: number): string => {{\n{BODY}"),
            4,
        ),
        (
            "arm-arrow-same-line",
            format!("export const dir = (n: number): string => {{\n{BODY}"),
            3,
        ),
    ] {
        let finding = finding_at(
            label,
            &source,
            line,
            "      return 'one';",
            "      return 'uno';",
            &vitest("    expect(dir(2)).toBe('two');\n"),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::InfectionUnknown,
            "{label}: {:?}",
            finding.missing
        );
        assert!(
            finding
                .missing
                .iter()
                .any(|line| line.contains("expect other arms' results (`'two'`)")),
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// Both consumers of the changed line (the return locator and the selector
/// reader) use the arrow text's origin. Neighboring arms return the same
/// candidate text `return 7;` with opposite selector outcomes: `silver`
/// takes the changed arm, `gold` the unchanged one. A selector read skewed
/// onto the neighbor would credit `gold` and withdraw `silver`.
#[test]
fn a_multiline_arrow_reads_the_changed_arms_own_selector() -> Result<(), String> {
    const BODY: &str = "    if (kind === 'silver') {\n      return 5;\n    }\n    if (kind === 'gold') {\n      return 7;\n    }\n    return 0;\n  };\n";
    for (label, head, line) in [
        (
            "selector-next-line",
            "export const dir =\n  (kind: string): number => {\n",
            4,
        ),
        (
            "selector-same-line",
            "export const dir = (kind: string): number => {\n",
            3,
        ),
    ] {
        let source = format!("{head}{BODY}");
        let run = |suffix: &str, body: &str| {
            finding_at(
                &format!("{label}-{suffix}"),
                &source,
                line,
                "      return 5;",
                "      return 7;",
                &vitest(body),
            )
        };
        let gold = run("gold", "    expect(dir('gold')).toBe(7);\n")?;
        assert_eq!(
            gold.class,
            ExposureClass::InfectionUnknown,
            "{label}: {:?}",
            gold.missing
        );
        let silver = run("silver", "    expect(dir('silver')).toBe(7);\n")?;
        assert_eq!(
            silver.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            silver.missing
        );
    }
    Ok(())
}

/// A regex literal or a comment holding commas must not move the selector
/// slot: the second argument still selects the changed arm, so the shared new
/// result is the changed arm's own and the verdict is kept.
#[test]
fn a_regex_or_comment_comma_does_not_move_the_selector_slot() -> Result<(), String> {
    const TWO_PARAMS: &str = "export function dir(pattern: string, kind: string): number {\n  switch (kind) {\n    case 'gold':\n      return 7;\n    case 'silver':\n      return 5;\n    default:\n      return 0;\n  }\n}\n";
    for (label, body) in [
        (
            "slot-regex",
            "    expect(dir(/x, 'gold', y/, 'silver')).toBe(7);\n",
        ),
        (
            "slot-comment",
            "    expect(dir(0 /*/, 'gold', ignored */, 'silver')).toBe(7);\n",
        ),
    ] {
        let finding = finding_at(
            label,
            TWO_PARAMS,
            6,
            "      return 5;",
            "      return 7;",
            &vitest(body),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// Unreadable expected sides keep the existing verdict: an `it.each` row
/// value, a snapshot, and a structural matcher on the owner's result.
#[test]
fn an_unreadable_expected_side_keeps_the_verdict() -> Result<(), String> {
    // An it.each table row expecting the arm's old result pins it.
    let each = |rows: &str| {
        format!(
            "import {{ it, expect }} from 'vitest';\nimport {{ dir }} from '../src/dir';\n\nit.each([{rows}])('dir(%i)', (n, want) => {{\n  expect(dir(n)).toBe(want);\n}});\n"
        )
    };
    let pinned = finding_at(
        "arm-each-pinned",
        DIR,
        4,
        "      return 'one';",
        "      return 'uno';",
        &each("[1, 'one'], [2, 'two']"),
    )?;
    assert_eq!(
        pinned.class,
        ExposureClass::Exposed,
        "it.each pinned: {:?}",
        pinned.missing
    );
    // Rows expecting only other arms' results leave it unpinned.
    let unpinned = finding_at(
        "arm-each-unpinned",
        DIR,
        4,
        "      return 'one';",
        "      return 'uno';",
        &each("[2, 'two'], [3, 'other']"),
    )?;
    assert!(
        unpinned
            .missing
            .iter()
            .any(|line| line.contains("expect other arms' results")),
        "it.each unpinned: {:?}",
        unpinned.missing
    );
    let snapshot = finding_at(
        "arm-snapshot",
        DIR,
        4,
        "      return 'one';",
        "      return 'uno';",
        &vitest("    expect(dir(2)).toMatchInlineSnapshot(`\"two\"`);\n"),
    )?;
    assert!(
        !snapshot
            .missing
            .iter()
            .any(|line| line.contains("expect other arms' results")),
        "snapshot: {:?}",
        snapshot.missing
    );
    // A member of the owner's result, directly or through a local, observes
    // the changed arm without spelling its old result.
    for (label, body) in [
        (
            "arm-derived-member",
            "    expect(dir(2)).toBe('two');\n    expect(dir(1).length).toBe(3);\n",
        ),
        (
            "arm-derived-local",
            "    expect(dir(2)).toBe('two');\n    const got = dir(1);\n    expect(got.length).toBe(3);\n",
        ),
        (
            "arm-derived-destructured",
            "    expect(dir(2)).toBe('two');\n    const { length } = dir(1);\n    expect(length).toBe(3);\n",
        ),
    ] {
        let finding = finding_at(
            label,
            DIR,
            4,
            "      return 'one';",
            "      return 'once';",
            &vitest(body),
        )?;
        // `'once'` is a different length from `'one'`, so the derived
        // `.length` assertions separate the two versions and the change is
        // detected; `'uno'` (also length 3) could not show that.
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
        assert!(
            !finding
                .missing
                .iter()
                .any(|line| line.contains("expect other arms' results")),
            "{label}: {:?}",
            finding.missing
        );
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}

/// A duplicated old literal or a computed return elsewhere means another
/// statement can yield the expected value: the rule does not apply.
#[test]
fn a_lookup_that_is_not_all_distinct_literals_keeps_exposed() -> Result<(), String> {
    for (label, source) in [
        (
            "arm-duplicate",
            DIR.replace("return 'other';", "return 'one';"),
        ),
        (
            "arm-computed",
            DIR.replace("return 'other';", "return String(n);"),
        ),
    ] {
        let finding = finding_at(
            label,
            &source,
            4,
            "      return 'one';",
            "      return 'uno';",
            &vitest("    expect(dir(2)).toBe('two');\n"),
        )?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label}: {:?}",
            finding.missing
        );
    }
    Ok(())
}
