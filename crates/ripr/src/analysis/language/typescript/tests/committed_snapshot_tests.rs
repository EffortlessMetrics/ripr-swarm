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

/// Write `files` under a fresh root (with a `.git` marker, so the config walk
/// has a repository boundary), extract `test/price.test.ts`, promote,
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
        .and_then(|()| ts_write_file(&root.join(".git/HEAD"), "x\n"))
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

#[test]
fn generic_or_bracket_snapshot_calls_are_counted_and_refuse() -> Result<(), String> {
    // #8031: a second call the counter sees must not hide behind its spelling.
    let snap = jest_snap(&[("price 1", "140")]);
    for second in [
        "expect(price(90)).toMatchSnapshot<number>();",
        "expect(price(90))[\"toMatchSnapshot\"]();",
        "expect(price(90)).toMatchSnapshot.call(this);",
    ] {
        let source = format!(
            "import {{ price }} from \"../src/price\";\n\ntest(\"price\", () => {{\n  expect(price(150)).toMatchSnapshot();\n  {second}\n}});\n"
        );
        let got = strengths("snap-generic", &source, &[(SNAP_PATH, &snap)])?;
        assert!(
            got.iter()
                .all(|strength| *strength != OracleStrength::Strong),
            "{second}: {got:?}"
        );
    }
    Ok(())
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
            .and_then(|()| ts_write_file(&root.join(".git/HEAD"), "x\n"))
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
            "{ \"scripts\": { \"test\": \"jest\t-u\" } }",
            &jest,
        ),
        (
            "jest.config.js",
            "module.exports = { snapshotFormat: { escapeString: false } };",
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

/// Promote `test/price.test.ts` under `package_root` (which may sit inside a
/// larger tree) and return the assertion strengths.
fn strengths_at(package_root: &Path) -> Vec<OracleStrength> {
    let relative = Path::new("test/price.test.ts");
    let mut tests = extract_tests(relative, PRICE_TEST);
    promote_committed_snapshots(package_root, relative, PRICE_TEST, &mut tests);
    tests
        .iter()
        .flat_map(|test| &test.assertions)
        .filter(|assertion| assertion.matcher == "toMatchSnapshot")
        .map(|assertion| assertion.oracle_strength.clone())
        .collect()
}

const RESOLVER_CONFIG: &str = "module.exports = { snapshotResolver: \"./r.js\" };";

#[test]
fn config_passed_by_path_or_custom_named_stays_medium() -> Result<(), String> {
    let snap = jest_snap(&[("price 1", "140")]);
    for (config, text) in [
        (
            "package.json",
            "{ \"scripts\": { \"test\": \"jest --config config/jest.js\" } }",
        ),
        (
            "package.json",
            "{ \"scripts\": { \"test\": \"jest --config=config/jest.js\" } }",
        ),
        (
            "package.json",
            "{ \"scripts\": { \"test\": \"jest -c config/jest.js\" } }",
        ),
        (
            "package.json",
            "{ \"scripts\": { \"unit\": \"vitest --config vitest.unit.ts\" } }",
        ),
        ("jest.unit.config.js", RESOLVER_CONFIG),
        (
            "vitest.unit.config.ts",
            "export default { test: { update: true } };",
        ),
    ] {
        assert_eq!(
            strengths(
                "snap-by-path",
                PRICE_TEST,
                &[(SNAP_PATH, &snap), (config, text)]
            )?,
            vec![OracleStrength::Medium],
            "{config}: {text}"
        );
    }
    // A custom-named config that overrides nothing and a script without a
    // config flag keep the credit. `-` and `_` stem separators are read the
    // same way as `.` (`jest.unit.config.js`).
    for (config, text) in [
        ("jest.unit.config.js", "module.exports = {};"),
        ("jest_unit.config.js", "module.exports = {};"),
        (
            "package.json",
            "{ \"scripts\": { \"test\": \"jest --ci --coverage\" } }",
        ),
    ] {
        assert_eq!(
            strengths(
                "snap-no-path",
                PRICE_TEST,
                &[(SNAP_PATH, &snap), (config, text)]
            )?,
            vec![OracleStrength::Strong],
            "{config}: {text}"
        );
    }
    // A Vitest custom-named config that overrides nothing keeps the credit.
    // `unit-vitest.config.ts` must mention vitest in the file (the name does
    // not start with `vite`, which is how `mentions_vitest` is set).
    let vitest_snap = format!("{VITEST_HEADER}{}", entry("price 1", "140"));
    let vitest_plain = "import { defineConfig } from \"vitest/config\";\nexport default defineConfig({ test: {} });";
    for name in ["vitest.unit.config.ts", "unit-vitest.config.ts"] {
        assert_eq!(
            strengths(
                "snap-vitest-custom",
                PRICE_TEST,
                &[(SNAP_PATH, &vitest_snap), (name, vitest_plain)]
            )?,
            vec![OracleStrength::Strong],
            "{name}"
        );
    }
    // The same `-` / `_` stems with an override refuse, so the walk did read
    // them (a name the walk skipped would keep Strong).
    assert_eq!(
        strengths(
            "snap-underscore-override",
            PRICE_TEST,
            &[(SNAP_PATH, &snap), ("jest_unit.config.js", RESOLVER_CONFIG)]
        )?,
        vec![OracleStrength::Medium]
    );
    assert_eq!(
        strengths(
            "snap-hyphen-override",
            PRICE_TEST,
            &[
                (SNAP_PATH, &vitest_snap),
                (
                    "unit-vitest.config.ts",
                    "import { defineConfig } from \"vitest/config\";\nexport default defineConfig({ test: { update: true } });"
                )
            ]
        )?,
        vec![OracleStrength::Medium]
    );
    Ok(())
}

/// A package at `packages/app` under a temp tree, with the snapshot and test
/// written, and `marker` (when present) created at the tree's top.
fn monorepo(label: &str, marker: Option<&str>) -> Result<(PathBuf, PathBuf), String> {
    let outer = ts_unique_tempdir(label)?;
    let package_root = outer.join("packages/app");
    let snap = jest_snap(&[("price 1", "140")]);
    ts_write_file(&package_root.join(SNAP_PATH), &snap)?;
    ts_write_file(&package_root.join("test/price.test.ts"), PRICE_TEST)?;
    if let Some(marker) = marker {
        ts_write_file(&outer.join(marker), "x\n")?;
    }
    Ok((outer, package_root))
}

#[test]
fn config_above_the_analysis_root_is_read_up_to_the_repository() -> Result<(), String> {
    let (outer, package_root) = monorepo("snap-above-root", Some(".git/HEAD"))?;
    let plain = strengths_at(&package_root);
    let written = ts_write_file(&outer.join("jest.config.js"), RESOLVER_CONFIG);
    let moved = strengths_at(&package_root);
    let _ = std::fs::remove_dir_all(&outer);
    written?;
    assert_eq!(plain, vec![OracleStrength::Strong]);
    assert_eq!(moved, vec![OracleStrength::Medium]);
    Ok(())
}

#[test]
fn directories_above_the_root_stop_at_the_repository_boundary() -> Result<(), String> {
    let (outer, package_root) = monorepo("snap-boundary", Some(".git/HEAD"))?;
    let outer = outer.canonicalize().map_err(|err| err.to_string())?;
    let reached = directories_above_root(&package_root, 16);
    // A marker beyond the look-up limit, with the filesystem root also
    // beyond it, leaves the boundary unknown.
    let beyond = directories_above_root(&package_root, 1);
    // The repository top itself has nothing above it to read.
    let top = directories_above_root(&outer, 16);
    let _ = std::fs::remove_dir_all(&outer);
    assert_eq!(reached, Some(vec![outer.join("packages"), outer.clone()]));
    assert_eq!(beyond, None);
    assert_eq!(top, Some(Vec::new()));
    Ok(())
}

/// With no marker anywhere, a walk that reaches the filesystem root within
/// the limit returns every ancestor (nothing above is a repository); one that
/// runs out of limit first has no known boundary.
#[test]
fn walk_without_a_marker_ends_at_the_filesystem_root_or_is_unknown() -> Result<(), String> {
    let (outer, package_root) = monorepo("snap-no-marker", None)?;
    let real = package_root.canonicalize().map_err(|err| err.to_string())?;
    let expected: Vec<PathBuf> = real.ancestors().skip(1).map(Path::to_path_buf).collect();
    let all = directories_above_root(&package_root, 4096);
    let short = directories_above_root(&package_root, 1);
    let _ = std::fs::remove_dir_all(&outer);
    // A marker on the machine above the temp dir would end the walk early;
    // either way the filesystem-root case is `Some`, never `None`.
    assert!(all.is_some(), "{all:?}");
    if !expected.iter().any(|dir| dir.join(".git").exists()) {
        assert_eq!(all, Some(expected));
    }
    assert_eq!(short, None);
    Ok(())
}

/// The credit for `test/price.test.ts` under `package_root` when the walk
/// above the root may climb `limit` levels.
fn configs_readable_within(package_root: &Path, limit: usize) -> Option<()> {
    runner_configs_within(package_root, Path::new("test"), limit).map(|_| ())
}

/// RIPR-SPEC-0258 example 7, end to end through the config walk: a monorepo
/// config with a `snapshotResolver` above the root refuses when the walk
/// reaches the repository marker, and refuses too when the marker and the
/// filesystem root are both beyond the look-up limit (boundary unknown).
#[test]
fn config_above_the_root_is_read_only_within_reach_of_the_marker() -> Result<(), String> {
    let (outer, package_root) = monorepo("snap-reach", Some(".git/HEAD"))?;
    let written = ts_write_file(&outer.join("jest.config.js"), RESOLVER_CONFIG);
    let reached = configs_readable_within(&package_root, 16);
    let out_of_reach = configs_readable_within(&package_root, 1);
    let _ = std::fs::remove_dir_all(&outer);
    written?;
    assert!(reached.is_none(), "boundary in reach: refuse");
    assert!(out_of_reach.is_none(), "boundary unknown: refuse");
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_analysis_root_reads_the_real_parents() -> Result<(), String> {
    let (outer, package_root) = monorepo("snap-root-link", Some(".git/HEAD"))?;
    let link_parent = ts_unique_tempdir("snap-root-link-parent")?;
    let link = link_parent.join("pkg-link");
    let linked = std::os::unix::fs::symlink(&package_root, &link).map_err(|err| err.to_string());
    let plain = strengths_at(&link);
    let written = ts_write_file(&outer.join("jest.config.js"), RESOLVER_CONFIG);
    let moved = strengths_at(&link);
    let _ = std::fs::remove_dir_all(&outer);
    let _ = std::fs::remove_dir_all(&link_parent);
    linked?;
    written?;
    assert_eq!(plain, vec![OracleStrength::Strong]);
    assert_eq!(moved, vec![OracleStrength::Medium]);
    Ok(())
}

#[test]
fn flags_and_keys_that_move_the_runner_config_stay_medium() -> Result<(), String> {
    let snap = jest_snap(&[("price 1", "140")]);
    for text in [
        "{ \"scripts\": { \"test\": \"jest -cconfig/jest.js\" } }",
        "{ \"scripts\": { \"test\": \"jest --rootDir src\" } }",
        "{ \"scripts\": { \"test\": \"vitest --root pkg\" } }",
        "{ \"scripts\": { \"test\": \"vitest -r pkg\" } }",
        "{ \"scripts\": { \"test\": \"vitest --dir pkg\" } }",
        "{ \"scripts\": { \"test\": \"vitest --workspace w.ts\" } }",
        "{ \"jest\": { \"projects\": [\"<rootDir>/a\"] } }",
        "{ \"jest\": { \"rootDir\": \"src\" } }",
    ] {
        assert_eq!(
            strengths(
                "snap-flags",
                PRICE_TEST,
                &[(SNAP_PATH, &snap), ("package.json", text)]
            )?,
            vec![OracleStrength::Medium],
            "{text}"
        );
    }
    // Names that merely contain a runner's letters are not runner configs, so
    // an override-looking word in them does not refuse.
    for name in ["invited.config.json", "service.config.ts"] {
        assert_eq!(
            strengths(
                "snap-not-runner",
                PRICE_TEST,
                &[(SNAP_PATH, &snap), (name, RESOLVER_CONFIG)]
            )?,
            vec![OracleStrength::Strong],
            "{name}"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_test_directory_stays_medium() -> Result<(), String> {
    let (outer, package_root) = monorepo("snap-symlink-dir", Some(".git/HEAD"))?;
    let plain = strengths_at(&package_root);
    // A symlinked analysis root is the user's own checkout path and is kept.
    let root_link = outer.join("pkg-link");
    let root_linked =
        std::os::unix::fs::symlink(&package_root, &root_link).map_err(|err| err.to_string());
    let through_root_link = strengths_at(&root_link);
    // A symlinked directory inside the root is refused.
    let real_test = outer.join("real-test");
    let test_dir = package_root.join("test");
    let swapped = std::fs::rename(&test_dir, &real_test)
        .and_then(|()| std::os::unix::fs::symlink(&real_test, &test_dir))
        .map_err(|err| err.to_string());
    let through_test_link = strengths_at(&package_root);
    let _ = std::fs::remove_dir_all(&outer);
    root_linked?;
    swapped?;
    assert_eq!(plain, vec![OracleStrength::Strong]);
    assert_eq!(through_root_link, vec![OracleStrength::Strong]);
    assert_eq!(through_test_link, vec![OracleStrength::Medium]);
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_snapshots_directory_stays_medium() -> Result<(), String> {
    let (outer, package_root) = monorepo("snap-symlink-snapdir", Some(".git/HEAD"))?;
    let plain = strengths_at(&package_root);
    let snapshots = package_root.join("test/__snapshots__");
    let real_snapshots = outer.join("real-snapshots");
    let swapped = std::fs::rename(&snapshots, &real_snapshots)
        .and_then(|()| std::os::unix::fs::symlink(&real_snapshots, &snapshots))
        .map_err(|err| err.to_string());
    let through_link = strengths_at(&package_root);
    let _ = std::fs::remove_dir_all(&outer);
    swapped?;
    assert_eq!(plain, vec![OracleStrength::Strong]);
    assert_eq!(through_link, vec![OracleStrength::Medium]);
    Ok(())
}

/// RIPR-SPEC-0258 example 7: with no repository marker and no filesystem root
/// within the 16-level reach, the boundary is unknown and the credit is
/// medium, end to end through `promote_committed_snapshots`.
#[test]
fn config_above_the_root_without_a_repository_boundary_is_not_read() -> Result<(), String> {
    let outer = ts_unique_tempdir("snap-deep-no-marker")?;
    let mut package_root = outer.clone();
    for level in 0..18 {
        package_root = package_root.join(format!("d{level}"));
    }
    let snap = jest_snap(&[("price 1", "140")]);
    let written = ts_write_file(&package_root.join(SNAP_PATH), &snap)
        .and_then(|()| ts_write_file(&package_root.join("test/price.test.ts"), PRICE_TEST));
    let deep = strengths_at(&package_root);
    // Positive control: the same package with a marker at its top is strong.
    let marked = ts_write_file(&package_root.join(".git/HEAD"), "x\n");
    let with_marker = strengths_at(&package_root);
    let _ = std::fs::remove_dir_all(&outer);
    written?;
    marked?;
    assert_eq!(
        deep,
        vec![OracleStrength::Medium],
        "boundary unknown: medium"
    );
    assert_eq!(with_marker, vec![OracleStrength::Strong]);
    Ok(())
}

/// RIPR-SPEC-0258 / #8768: a runner config that names another config
/// (`projects`, `workspace`, or `root`) is not walked, so the reading
/// refuses. A `rootDir` / `<rootDir>` mention is not that key: a broad
/// `root` substring would refuse the TypeScript verdict corpus alias and
/// monorepo subjects.
#[test]
fn runner_config_that_delegates_stays_medium() -> Result<(), String> {
    let jest = jest_snap(&[("price 1", "140")]);
    let vitest = format!("{VITEST_HEADER}{}", entry("price 1", "140"));
    for (config, text, snap) in [
        (
            "jest.config.js",
            "module.exports = { projects: [\"<rootDir>/config/app.jest.js\"] };",
            &jest,
        ),
        (
            "jest.config.js",
            "module.exports = { \"projects\": [\"<rootDir>/config/app.jest.js\"] };",
            &jest,
        ),
        (
            "vitest.config.ts",
            "export default { test: { workspace: [\"./packages/*\"] } };",
            &vitest,
        ),
        (
            "vitest.config.ts",
            "export default { workspace: [\"./packages/*\"] };",
            &vitest,
        ),
        (
            "vitest.config.ts",
            "export default { test: { root: \"./pkg\" } };",
            &vitest,
        ),
        (
            "vitest.config.ts",
            "export default { root: \"./pkg\" };",
            &vitest,
        ),
        (
            "vite.config.ts",
            "export default { test: { root: \"./pkg\" } };",
            &vitest,
        ),
    ] {
        assert_eq!(
            strengths(
                "snap-delegate",
                PRICE_TEST,
                &[(SNAP_PATH, snap), (config, text)]
            )?,
            vec![OracleStrength::Medium],
            "{config}: {text}"
        );
    }
    // `<rootDir>` in a mapper, and Jest's `rootDir` key, do not delegate to
    // another config file. Matching the `root` substring would refuse these.
    for (config, text) in [
        (
            "jest.config.js",
            "module.exports = { moduleNameMapper: { \"^@/(.*)$\": \"<rootDir>/src/$1\" } };",
        ),
        ("jest.config.js", "module.exports = { rootDir: \"src\" };"),
    ] {
        assert_eq!(
            strengths(
                "snap-rootdir",
                PRICE_TEST,
                &[(SNAP_PATH, &jest), (config, text)]
            )?,
            vec![OracleStrength::Strong],
            "{config}: {text}"
        );
    }
    Ok(())
}

/// A `.git` *file* (worktree `gitdir:` pointer) is a repository boundary,
/// the same as a `.git` directory. A parent config above that file is not
/// read.
#[test]
fn git_file_is_a_repository_boundary() -> Result<(), String> {
    let (outer, package_root) = monorepo("snap-git-file", None)?;
    let written = ts_write_file(
        &package_root.join(".git"),
        "gitdir: ../.git/worktrees/app\n",
    )
    .and_then(|()| ts_write_file(&outer.join("jest.config.js"), RESOLVER_CONFIG));
    let above = directories_above_root(&package_root, 16);
    let got = strengths_at(&package_root);
    let _ = std::fs::remove_dir_all(&outer);
    written?;
    assert_eq!(
        above,
        Some(Vec::new()),
        ".git file at the analysis root: nothing above is read"
    );
    assert_eq!(
        got,
        vec![OracleStrength::Strong],
        "parent snapshotResolver stays unseen when .git is a file at the root"
    );
    Ok(())
}
