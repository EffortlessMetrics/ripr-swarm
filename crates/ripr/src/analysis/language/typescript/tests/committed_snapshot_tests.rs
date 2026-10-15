//! RIPR-SPEC-0258: a committed snapshot entry the assertion compares with
//! reads `snapshot` / strong; every shape whose entry cannot be named keeps
//! `snapshot` / medium.

use super::*;

const JEST_HEADER: &str = "// Jest Snapshot v1, https://jestjs.io/docs/snapshot-testing\n\n";
const VITEST_HEADER: &str = "// Vitest Snapshot v1, https://vitest.dev/guide/snapshot.html\n\n";

const PRICE_TEST: &str = "import { price } from \"../src/price\";\n\ntest(\"price\", () => {\n  expect(price(150)).toMatchSnapshot();\n});\n";

fn entry(key: &str, value: &str) -> String {
    format!("exports[`{key}`] = `{value}`;\n\n")
}

/// Write `files` under a fresh root, extract `test/price.test.ts`, promote,
/// and return each `toMatchSnapshot` assertion's strength in test order.
fn strengths(
    label: &str,
    test_source: &str,
    files: &[(&str, &str)],
) -> Result<Vec<OracleStrength>, String> {
    let root = ts_unique_tempdir(label)?;
    let written = files
        .iter()
        .try_for_each(|(path, contents)| ts_write_file(&root.join(path), contents))
        .and_then(|()| ts_write_file(&root.join("test/price.test.ts"), test_source));
    let relative = Path::new("test/price.test.ts");
    let mut tests = extract_tests(relative, test_source);
    if written.is_ok() {
        promote_committed_snapshots(&root, relative, test_source, &mut tests);
    }
    let _ = std::fs::remove_dir_all(&root);
    written?;
    Ok(tests
        .iter()
        .flat_map(|test| &test.assertions)
        .filter(|assertion| assertion.matcher == "toMatchSnapshot")
        .map(|assertion| assertion.oracle_strength.clone())
        .collect())
}

fn jest_snap(entries: &[(&str, &str)]) -> String {
    let body: String = entries
        .iter()
        .map(|(key, value)| entry(key, value))
        .collect();
    format!("{JEST_HEADER}{body}")
}

const SNAP_PATH: &str = "test/__snapshots__/price.test.ts.snap";

#[test]
fn committed_jest_snapshot_entry_reads_strong() -> Result<(), String> {
    let snap = jest_snap(&[("price 1", "140")]);
    assert_eq!(
        strengths("snap-jest", PRICE_TEST, &[(SNAP_PATH, &snap)])?,
        vec![OracleStrength::Strong]
    );
    Ok(())
}

#[test]
fn committed_snapshot_keys_join_describe_titles_per_runner() -> Result<(), String> {
    let test = "describe(\"price\", () => {\n  it(\"caps\", () => {\n    expect(price(150)).toMatchSnapshot();\n  });\n});\n";
    let jest = jest_snap(&[("price caps 1", "140")]);
    assert_eq!(
        strengths("snap-describe-jest", test, &[(SNAP_PATH, &jest)])?,
        vec![OracleStrength::Strong]
    );
    let vitest = format!("{VITEST_HEADER}{}", entry("price > caps 1", "140"));
    let vitest_config = ("vitest.config.ts", "export default { test: {} };");
    assert_eq!(
        strengths(
            "snap-describe-vitest",
            test,
            &[(SNAP_PATH, &vitest), vitest_config]
        )?,
        vec![OracleStrength::Strong]
    );
    // Each runner's own separator only: a Vitest file keyed the Jest way
    // holds no entry this assertion compares with.
    let crossed = format!("{VITEST_HEADER}{}", entry("price caps 1", "140"));
    assert_eq!(
        strengths(
            "snap-describe-crossed",
            test,
            &[(SNAP_PATH, &crossed), vitest_config]
        )?,
        vec![OracleStrength::Medium]
    );
    Ok(())
}

#[test]
fn snapshot_without_its_committed_entry_stays_medium() -> Result<(), String> {
    let medium = vec![OracleStrength::Medium];
    // No file, another test's entry, an unknown header.
    assert_eq!(strengths("snap-none", PRICE_TEST, &[])?, medium);
    let other = jest_snap(&[("total 1", "140")]);
    assert_eq!(
        strengths("snap-other", PRICE_TEST, &[(SNAP_PATH, &other)])?,
        medium
    );
    let unknown = format!("// Snapshot v2\n\n{}", entry("price 1", "140"));
    assert_eq!(
        strengths("snap-header", PRICE_TEST, &[(SNAP_PATH, &unknown)])?,
        medium
    );
    // An entry the runner did not write (here, in a comment) is not one.
    let commented = format!("{JEST_HEADER}// {}", entry("price 1", "140"));
    assert_eq!(
        strengths("snap-commented", PRICE_TEST, &[(SNAP_PATH, &commented)])?,
        medium
    );
    // A second entry means the assertion ran twice when the file was written.
    let twice = jest_snap(&[("price 1", "140"), ("price 2", "150")]);
    assert_eq!(
        strengths("snap-twice", PRICE_TEST, &[(SNAP_PATH, &twice)])?,
        medium
    );
    Ok(())
}

#[test]
fn snapshot_whose_counter_cannot_be_named_stays_medium() -> Result<(), String> {
    let snap = jest_snap(&[("price 1", "140")]);
    let files = [(SNAP_PATH, snap.as_str())];
    let medium = |count: usize| vec![OracleStrength::Medium; count];
    // Two snapshot calls in the body, counting an inline snapshot.
    let two = "test(\"price\", () => {\n  expect(price(150)).toMatchSnapshot();\n  expect(price(90)).toMatchInlineSnapshot(`90`);\n});\n";
    assert_eq!(strengths("snap-two-calls", two, &files)?, medium(1));
    // A hook advances the counter outside the test body.
    let hook = "afterEach(() => {\n  expect(log()).toMatchSnapshot();\n});\ntest(\"price\", () => {\n  expect(price(150)).toMatchSnapshot();\n});\n";
    assert_eq!(strengths("snap-hook", hook, &files)?, medium(1));
    // Two tests sharing a title share a counter.
    let shared = "test(\"price\", () => {\n  expect(price(150)).toMatchSnapshot();\n});\ntest(\"price\", () => {\n  expect(price(90)).toMatchSnapshot();\n});\n";
    assert_eq!(strengths("snap-shared", shared, &files)?, medium(2));
    // A hint changes the key.
    let hint = "test(\"price\", () => {\n  expect(price(150)).toMatchSnapshot(\"cap\");\n});\n";
    assert_eq!(strengths("snap-hint", hint, &files)?, medium(1));
    // Whitespace before `(` still calls the matcher and advances the counter.
    let spaced = "test(\"price\", () => {\n  expect(1).toMatchSnapshot ();\n  expect(price(150)).toMatchSnapshot();\n});\n";
    assert_eq!(strengths("snap-spaced", spaced, &files)?, medium(2));
    // Titles in two describes that join to the same key share its counter.
    let joined = "describe(\"a b\", () => {\n  test(\"c\", () => {\n    expect(price(150)).toMatchSnapshot();\n  });\n});\ndescribe(\"a\", () => {\n  test(\"b c\", () => {\n    expect(price(90)).toMatchSnapshot();\n  });\n});\n";
    let joined_snap = jest_snap(&[("a b c 1", "140")]);
    assert_eq!(
        strengths("snap-joined", joined, &[(SNAP_PATH, &joined_snap)])?,
        medium(2)
    );
    // Concurrent tests may record a snapshot under another test's title.
    let concurrent =
        "test.concurrent(\"price\", async () => {\n  expect(price(150)).toMatchSnapshot();\n});\n";
    assert_eq!(strengths("snap-concurrent", concurrent, &files)?, medium(1));
    // The file escapes these characters in its keys, and an empty title has
    // no key of its own. The file holds the title both raw and as written in
    // the source, so only the title guard refuses.
    for (title, raw) in [
        ("pri`ce", "pri`ce"),
        ("pri$ce", "pri$ce"),
        ("pri\\\\ce", "pri\\ce"),
        ("", ""),
    ] {
        let test =
            format!("test(\"{title}\", () => {{\n  expect(price(150)).toMatchSnapshot();\n}});\n");
        let written = format!("{title} 1");
        let unescaped = format!("{raw} 1");
        let mut keys = vec![(written.as_str(), "140")];
        if unescaped != written {
            keys.push((unescaped.as_str(), "140"));
        }
        let snap = jest_snap(&keys);
        assert_eq!(
            strengths("snap-escaped", &test, &[(SNAP_PATH, &snap)])?,
            medium(1),
            "{title}"
        );
    }
    Ok(())
}

#[test]
fn snapshot_input_differing_from_head_stays_medium() -> Result<(), String> {
    // A committed-history run reads the test at `HEAD`: a `.snap` file or
    // runner config that differs there (edited, untracked, ignored) is not
    // what `HEAD` holds.
    let snap = jest_snap(&[("price 1", "140")]);
    for (label, dirty, expected) in [
        (
            "snap-clean-inputs",
            "src/__snapshots__/other.test.ts.snap",
            OracleStrength::Strong,
        ),
        ("snap-dirty-entry", SNAP_PATH, OracleStrength::Medium),
        ("snap-dirty-config", "package.json", OracleStrength::Medium),
        (
            "snap-dirty-test-config",
            "test/jest.config.js",
            OracleStrength::Medium,
        ),
    ] {
        let root = ts_unique_tempdir(label)?;
        let written = ts_write_file(&root.join(SNAP_PATH), &snap)
            .and_then(|()| ts_write_file(&root.join("test/price.test.ts"), PRICE_TEST));
        let relative = Path::new("test/price.test.ts");
        let mut tests = extract_tests(relative, PRICE_TEST);
        let overlay =
            crate::analysis::committed_source::CommittedSourceOverlay::from_entries(&root, [])
                .with_snapshot_inputs(&[dirty]);
        if written.is_ok() {
            crate::analysis::committed_source::with_overlay(
                Some(std::sync::Arc::new(overlay)),
                || promote_committed_snapshots(&root, relative, PRICE_TEST, &mut tests),
            );
        }
        let _ = std::fs::remove_dir_all(&root);
        written?;
        let strengths: Vec<OracleStrength> = tests
            .iter()
            .flat_map(|test| &test.assertions)
            .map(|assertion| assertion.oracle_strength.clone())
            .collect();
        assert_eq!(strengths, vec![expected], "{dirty}");
    }
    Ok(())
}

#[test]
fn snapshot_location_or_update_override_stays_medium() -> Result<(), String> {
    let jest = jest_snap(&[("price 1", "140")]);
    let vitest = format!("{VITEST_HEADER}{}", entry("price 1", "140"));
    for (config, text, snap) in [
        (
            "jest.config.js",
            "module.exports = { snapshotResolver: \"./resolver.js\" };",
            &jest,
        ),
        (
            "jest.config.js",
            "module.exports = { updateSnapshot: \"all\" };",
            &jest,
        ),
        (
            "package.json",
            "{ \"scripts\": { \"test\": \"jest -u\" } }",
            &jest,
        ),
        (
            "package.json",
            "{ \"scripts\": { \"test\": \"jest --updateSnapshot\" } }",
            &jest,
        ),
        (
            "package.json",
            "{ \"jest\": { \"args\": [\"-u\"] } }",
            &jest,
        ),
        (
            "test/package.json",
            "{ \"scripts\": { \"test\": \"vitest --update\" } }",
            &vitest,
        ),
        (
            "vitest.config.ts",
            "export default { test: { resolveSnapshotPath: (p) => p } };",
            &vitest,
        ),
        (
            "vitest.config.ts",
            "export default { test: { snapshotEnvironment: \"./env.ts\" } };",
            &vitest,
        ),
        (
            "vitest.config.ts",
            "export default { test: { update: true } };",
            &vitest,
        ),
        (
            "vitest.config.ts",
            "export default { test: { update : true } };",
            &vitest,
        ),
        (
            "vitest.config.ts",
            "export default { test: { \"update\": true } };",
            &vitest,
        ),
        (
            "vite.config.ts",
            "export default { test: { update: true } };",
            &vitest,
        ),
        (
            "vitest.workspace.ts",
            "export default [{ test: { resolveSnapshotPath: (p) => p } }];",
            &vitest,
        ),
        // The header names the runner that wrote the file: Vitest keys a
        // file Jest left with ` > `, and Jest the reverse.
        ("vitest.config.ts", "export default { test: {} };", &jest),
        (
            "package.json",
            "{ \"scripts\": { \"test\": \"jest\" } }",
            &vitest,
        ),
    ] {
        assert_eq!(
            strengths(
                "snap-override",
                PRICE_TEST,
                &[(SNAP_PATH, snap), (config, text)]
            )?,
            vec![OracleStrength::Medium],
            "{config}: {text}"
        );
    }
    // A config that only names the runner keeps the default location, and a
    // `package.json` script named `update` is not Vitest's option.
    for (config, text, snap) in [
        (
            "package.json",
            "{ \"scripts\": { \"test\": \"jest --ci\", \"update\": \"npm i\" } }",
            &jest,
        ),
        ("vitest.config.ts", "export default { test: {} };", &vitest),
    ] {
        assert_eq!(
            strengths(
                "snap-plain-config",
                PRICE_TEST,
                &[(SNAP_PATH, snap), (config, text)]
            )?,
            vec![OracleStrength::Strong],
            "{config}: {text}"
        );
    }
    Ok(())
}

fn hinted(hint: &str) -> String {
    format!("test(\"price\", () => {{\n  expect(price(150)).toMatchSnapshot({hint});\n}});\n")
}

#[test]
fn committed_jest_hint_entry_reads_strong() -> Result<(), String> {
    // Jest keys `toMatchSnapshot("cap")` as `<title>: cap <counter>`.
    let snap = jest_snap(&[("price: cap 1", "140")]);
    for hint in ["\"cap\"", "'cap'", " \"cap\" "] {
        assert_eq!(
            strengths("snap-hint-jest", &hinted(hint), &[(SNAP_PATH, &snap)])?,
            vec![OracleStrength::Strong],
            "{hint}"
        );
    }
    let described = "describe(\"d\", () => {\n  it(\"price\", () => {\n    expect(price(150)).toMatchSnapshot(\"cap more\");\n  });\n});\n";
    let described_snap = jest_snap(&[("d price: cap more 1", "140")]);
    assert_eq!(
        strengths(
            "snap-hint-describe",
            described,
            &[(SNAP_PATH, &described_snap)]
        )?,
        vec![OracleStrength::Strong]
    );
    // A test whose title only starts the key does not share its counter.
    let neighbour = format!(
        "{}test(\"pri\", () => {{\n  expect(price(9)).toMatchSnapshot();\n}});\n",
        hinted("\"cap\"")
    );
    let neighbour_snap = jest_snap(&[("price: cap 1", "140"), ("pri 1", "9")]);
    assert_eq!(
        strengths(
            "snap-hint-neighbour",
            &neighbour,
            &[(SNAP_PATH, &neighbour_snap)]
        )?,
        vec![OracleStrength::Strong; 2]
    );
    Ok(())
}

#[test]
fn hinted_snapshot_without_its_exact_entry_stays_medium() -> Result<(), String> {
    let medium = |count: usize| vec![OracleStrength::Medium; count];
    let cap = hinted("\"cap\"");
    for (label, entries) in [
        // A wrong hint, the unhinted key, a key without the `: ` joint.
        ("snap-hint-wrong", vec![("price: cape 1", "140")]),
        ("snap-hint-unhinted", vec![("price 1", "140")]),
        ("snap-hint-joint", vec![("price cap 1", "140")]),
        // A second entry means the call ran twice when the file was written.
        (
            "snap-hint-twice",
            vec![("price: cap 1", "140"), ("price: cap 2", "150")],
        ),
    ] {
        let snap = jest_snap(&entries);
        assert_eq!(
            strengths(label, &cap, &[(SNAP_PATH, &snap)])?,
            medium(1),
            "{label}"
        );
    }
    // A missing file.
    assert_eq!(strengths("snap-hint-none", &cap, &[])?, medium(1));
    // The entry under a hint that is not a plain string literal, or one the
    // file escapes or Jest drops.
    for (hint, raw) in [
        ("`cap`", "cap"),
        ("hint", "cap"),
        ("\"ca\" + \"p\"", "ca"),
        ("\"cap\", \"x\"", "cap"),
        ("/* c */ \"cap\"", "cap"),
        ("\"ca$p\"", "ca$p"),
        ("\"ca\\\\p\"", "ca\\p"),
        ("\"ca\\\\p\"", "ca\\\\p"),
        ("\"\"", ""),
    ] {
        let hinted_key = format!("price: {raw} 1");
        let snap = jest_snap(&[(hinted_key.as_str(), "140"), ("price 1", "140")]);
        assert_eq!(
            strengths("snap-hint-shape", &hinted(hint), &[(SNAP_PATH, &snap)])?,
            medium(1),
            "{hint}"
        );
    }
    // Vitest's hinted key is not read.
    let vitest = format!(
        "{VITEST_HEADER}{}{}",
        entry("price > cap 1", "140"),
        entry("price: cap 1", "140")
    );
    assert_eq!(
        strengths(
            "snap-hint-vitest",
            &cap,
            &[
                (SNAP_PATH, &vitest),
                ("vitest.config.ts", "export default { test: {} };")
            ]
        )?,
        medium(1)
    );
    Ok(())
}

#[test]
fn hinted_snapshot_sharing_its_counter_stays_medium() -> Result<(), String> {
    let medium = |count: usize| vec![OracleStrength::Medium; count];
    let snap = jest_snap(&[("price: cap 1", "140"), ("price 1", "90")]);
    let files = [(SNAP_PATH, snap.as_str())];
    // Two tests titled `price`: the other's call may use the same hint.
    let duplicate = "test(\"price\", () => {\n  expect(price(150)).toMatchSnapshot(\"cap\");\n});\ntest(\"price\", () => {\n  expect(price(90)).toMatchSnapshot();\n});\n";
    assert_eq!(strengths("snap-hint-dup", duplicate, &files)?, medium(2));
    // A test titled `price: cap` keys its unhinted call the same way.
    let titled = "test(\"price\", () => {\n  expect(price(150)).toMatchSnapshot(\"cap\");\n});\ntest(\"price: cap\", () => {\n  expect(price(90)).toMatchSnapshot();\n});\n";
    assert_eq!(strengths("snap-hint-titled", titled, &files)?, medium(2));
    // A second snapshot call in the body.
    let two = "test(\"price\", () => {\n  expect(price(150)).toMatchSnapshot(\"cap\");\n  expect(price(90)).toMatchSnapshot(\"cap\");\n});\n";
    assert_eq!(strengths("snap-hint-two", two, &files)?, medium(2));
    // A string naming the matcher next to a call the body count misses:
    // the hint must not be read from the string.
    let mentioned = "test(\"price\", () => {\n  const doc = 'x.toMatchSnapshot(\"cap\")';\n  expect(price(150)).toMatchSnapshot<number>(\"real\");\n});\n";
    assert_eq!(
        strengths("snap-hint-mentioned", mentioned, &files)?,
        medium(1)
    );
    // Vitest keys a hint `<title> > <hint>`, so a top-level `price` shares
    // the counter of `price > cap` even though its hint is not read.
    let vitest = format!("{VITEST_HEADER}{}", entry("price > cap 1", "140"));
    let vitest_files = [
        (SNAP_PATH, vitest.as_str()),
        ("vitest.config.ts", "export default { test: {} };"),
    ];
    let vitest_shared = "describe(\"price\", () => {\n  test(\"cap\", () => {\n    expect(price(150)).toMatchSnapshot();\n  });\n});\ntest(\"price\", () => {\n  expect(price(90)).toMatchSnapshot(\"cap\");\n});\n";
    assert_eq!(
        strengths("snap-hint-vitest-shared", vitest_shared, &vitest_files)?,
        medium(2)
    );
    Ok(())
}
