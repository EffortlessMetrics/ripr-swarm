//! RIPR-SPEC-0247 rule 4: a literal `toHaveBeenCalledWith` on a
//! `jest.spyOn` / `vi.spyOn` of an exported module object pins the changed
//! `object.method(...)` call; every way another call could reach the spy
//! keeps the `mock_expectation` / medium reading.

use super::*;

const OWNER: &str = "export const mailer = {\n  send(message: string): string {\n    return message;\n  },\n};\n\nexport function announceMail(id: number): void {\n  mailer.send(\"order \" + id);\n}\n";
const LINE: (usize, &str) = (8, "  mailer.send(\"order \" + id);");
const IMPORT: &str = "import { announceMail, mailer } from \"../src/mail\";\n";
const SPY_TEST: &str = "\ntest(\"announceMail\", () => {\n  const send = jest.spyOn(mailer, \"send\");\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n});\n";

/// The class of the `announceMail` finding for `owner` (changed `line`)
/// and the test file `test`.
fn spy_class(
    label: &str,
    owner: &str,
    line: (usize, &str),
    test: &str,
) -> Result<ExposureClass, String> {
    // A refusal that reads the wrong line proves nothing about its guard.
    if owner.lines().nth(line.0 - 1) != Some(line.1) {
        return Err(format!(
            "{label}: changed line {} is not `{}`",
            line.0, line.1
        ));
    }
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/mail.ts"), owner)?;
    ts_write_file(
        &root.join("src/other.ts"),
        "export const mailer = { send(m: string) { return m; } };\n",
    )?;
    ts_write_file(&root.join("tests/mail.test.ts"), test)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/mail.ts", &[line])],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("announceMail"))
        })
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding for `announceMail`"))
}

/// Checks every `(label, owner, line, test)` case and reports each miss, so
/// one regression does not hide another.
fn assert_all(
    cases: &[(&str, &str, (usize, &str), String)],
    expected: ExposureClass,
) -> Result<(), String> {
    let misses: Vec<String> = cases
        .iter()
        .filter_map(
            |(label, owner, line, test)| match spy_class(label, owner, *line, test) {
                Ok(class) if class == expected => None,
                Ok(class) => Some(format!("{label}: expected {expected:?}, got {class:?}")),
                Err(error) => Some(error),
            },
        )
        .collect();
    if misses.is_empty() {
        Ok(())
    } else {
        Err(misses.join("; "))
    }
}

/// The 1-based line of the changed call in `owner`, wherever the case's
/// edit moved it.
fn line_of(owner: &str) -> usize {
    owner
        .lines()
        .position(|line| line == LINE.1)
        .map_or(0, |at| at + 1)
}

fn test_with(body: &str) -> String {
    format!("{IMPORT}\ntest(\"announceMail\", () => {{\n{body}\n}});\n")
}

#[test]
fn spy_on_an_exported_module_object_the_changed_line_calls_is_credited() -> Result<(), String> {
    assert_all(
        &[
            ("spy-jest", OWNER, LINE, format!("{IMPORT}{SPY_TEST}")),
            (
                "spy-vi",
                OWNER,
                LINE,
                format!(
                    "import {{ expect, test, vi }} from \"vitest\";\n{IMPORT}\ntest(\"announceMail\", () => {{\n  const send = vi.spyOn(mailer, 'send');\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n}});\n"
                ),
            ),
            (
                "spy-renamed-import",
                OWNER,
                LINE,
                "import { announceMail, mailer as m } from \"../src/mail\";\ntest(\"announceMail\", () => {\n  const send = jest.spyOn(m, \"send\");\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n});\n"
                    .to_string(),
            ),
            (
                "spy-jest-globals",
                OWNER,
                LINE,
                format!("import {{ expect, jest, test }} from \"@jest/globals\";\n{IMPORT}{SPY_TEST}"),
            ),
            (
                "spy-it",
                OWNER,
                LINE,
                format!("{IMPORT}{}", SPY_TEST.replace("test(", "it(")),
            ),
        ],
        ExposureClass::Exposed,
    )
}

#[test]
fn spy_on_a_module_object_refuses_every_other_path_to_the_spy() -> Result<(), String> {
    let refused_owners = [
        (
            "owner-second-call",
            OWNER.replace(
                "  mailer.send(\"order \" + id);\n}",
                "  mailer.send(\"order \" + id);\n  log(id);\n}\nfunction log(id: number) {\n  mailer.send(\"order 7\");\n}",
            ),
        ),
        (
            "owner-second-mention",
            OWNER.replace(
                "  mailer.send(\"order \" + id);\n}",
                "  mailer.send(\"order \" + id);\n  void mailer;\n}",
            ),
        ),
        (
            "owner-param-call",
            OWNER.replace(
                "  mailer.send(\"order \" + id);\n}",
                "  mailer.send(\"order \" + id);\n  id.toFixed(0);\n}",
            ),
        ),
        (
            "owner-getter-read",
            OWNER.replace(
                "  mailer.send(\"order \" + id);\n}",
                "  mailer.send(\"order \" + id);\n  void config.suffix;\n}\nexport const config = { get suffix() { mailer.send(\"order 7\"); return \"\"; } };",
            ),
        ),
        (
            "owner-default",
            OWNER.replace("announceMail(id: number)", "announceMail(id: number, pre = warm())")
                + "function warm() {\n  mailer.send(\"order 7\");\n}\n",
        ),
        (
            "owner-non-ascii",
            OWNER.replace(
                "  mailer.send(\"order \" + id);\n}",
                "  mailer.send(\"order \" + id);\n  void \u{e9};\n}\nexport const \u{e9} = 1;",
            ),
        ),
        ("owner-let", OWNER.replace("export const mailer", "export let mailer")),
        (
            // The spy wraps the original method, which calls the spy again.
            "owner-method-self-call",
            OWNER.replace(
                "    return message;",
                "    if (message !== \"order 7\") this.send(\"order 7\");\n    return message;",
            ),
        ),
        (
            "owner-method-names-object",
            OWNER.replace("    return message;", "    void mailer;\n    return message;"),
        ),
        (
            "owner-method-helper",
            OWNER.replace("    return message;", "    return relay(message);")
                + "function relay(m: string): string {\n  return m;\n}\n",
        ),
        (
            "owner-method-module-word",
            OWNER.replace("    return message;", "    return message + suffix;")
                + "export const suffix = \"\";\n",
        ),
        (
            "owner-property-value",
            OWNER.replace(
                "  send(message: string): string {\n    return message;\n  },",
                "  send: relay,\n\n\n",
            ) + "function relay(m: string): string {\n  return m;\n}\n",
        ),
        (
            "owner-spread",
            OWNER.replace("export const mailer = {\n", "export const mailer = {\n  ...base,\n")
                + "const base = { send(m: string) { return m; } };\n",
        ),
        (
            "owner-getter-member",
            OWNER.replace(
                "  send(message: string): string {",
                "  get tag() { return \"\"; },\n  send(message: string): string {",
            )
            .replace("\n\nexport function", "\nexport function"),
        ),
        (
            // The method's default runs inside the spied call.
            "owner-method-default",
            OWNER.replace(
                "  send(message: string): string {",
                "  send(message: string, n = 0): string {",
            ),
        ),
        (
            // Module code outside the object can leave a mock with calls.
            "owner-module-reassigns",
            OWNER.to_string() + "mailer.send = jest.fn();\nmailer.send(\"order 7\");\n",
        ),
        (
            "owner-module-imports",
            OWNER.replace("export const mailer", "import \"./patch\";\nexport const mailer").replacen("\n\nexport function", "\nexport function", 1),
        ),
        (
            "owner-annotated",
            OWNER.replace("export const mailer = {", "export const mailer: Mailer = {")
                + "type Mailer = { send(m: string): string };\n",
        ),
        (
            "owner-not-an-object",
            OWNER.replace(
                "export const mailer = {\n  send(message: string): string {\n    return message;\n  },\n};",
                "export const mailer = makeMailer();\nfunction makeMailer() {\n  return { send(m: string) { return m; } };\n}",
            ),
        ),
        (
            "owner-unexported",
            OWNER.replace("export const mailer", "const mailer"),
        ),
        (
            "owner-imported-object",
            "import { mailer } from \"./other\";\n\n\n\n\n\nexport function announceMail(id: number): void {\n  mailer.send(\"order \" + id);\n}\n"
                .to_string(),
        ),
        (
            "owner-new",
            OWNER.replace(
                "  mailer.send(\"order \" + id);\n}",
                "  mailer.send(\"order \" + id);\n  void new Date;\n}",
            ),
        ),
        (
            // A getter read inside a template's `${...}`.
            "owner-template-getter",
            OWNER.replace(
                "  mailer.send(\"order \" + id);\n}",
                "  mailer.send(\"order \" + id);\n  void `${config.suffix}`;\n}\nexport const config = { get suffix() { mailer.send(\"order 7\"); return \"\"; } };",
            ),
        ),
    ];
    let mut cases: Vec<(&str, &str, (usize, &str), String)> = refused_owners
        .iter()
        .map(|(label, owner)| {
            (
                *label,
                owner.as_str(),
                (line_of(owner), LINE.1),
                format!("{IMPORT}{SPY_TEST}"),
            )
        })
        .collect();
    let refused_tests = [
        (
            "test-direct-call",
            test_with(
                "  const send = jest.spyOn(mailer, \"send\");\n  announceMail(8);\n  mailer.send(\"order 7\");\n  expect(send).toHaveBeenCalledWith(\"order 7\");",
            ),
        ),
        (
            "test-two-owner-calls",
            test_with(
                "  const send = jest.spyOn(mailer, \"send\");\n  announceMail(8);\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");",
            ),
        ),
        (
            "test-helper-call",
            format!(
                "{IMPORT}import {{ warm }} from \"../src/other\";\ntest(\"announceMail\", () => {{\n  const send = jest.spyOn(mailer, \"send\");\n  warm();\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n}});\n"
            ),
        ),
        (
            "test-spy-in-block",
            test_with(
                "  {\n    const send = jest.spyOn(mailer, \"send\");\n  }\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");",
            ),
        ),
        (
            "test-spy-mentioned",
            test_with(
                "  const send = jest.spyOn(mailer, \"send\");\n  announceMail(8);\n  send(\"order 7\");\n  expect(send).toHaveBeenCalledWith(\"order 7\");",
            ),
        ),
        (
            "test-tagged-owner",
            test_with(
                "  const send = jest.spyOn(mailer, \"send\");\n  announceMail(8);\n  announceMail`7`;\n  expect(send).toHaveBeenCalledWith(\"order 7\");",
            ),
        ),
        (
            "test-getter",
            format!(
                "{IMPORT}import {{ other }} from \"../src/other\";\ntest(\"announceMail\", () => {{\n  const send = jest.spyOn(mailer, \"send\");\n  other.warm;\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n}});\n"
            ),
        ),
        (
            "test-non-ascii-helper",
            format!(
                "{IMPORT}import {{ \u{e9} }} from \"../src/other\";\ntest(\"announceMail\", () => {{\n  const send = jest.spyOn(mailer, \"send\");\n  \u{e9}();\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n}});\n"
            ),
        ),
        (
            "test-await",
            format!(
                "{IMPORT}\ntest(\"announceMail\", async () => {{\n  const send = jest.spyOn(mailer, \"send\");\n  announceMail(7);\n  await 0;\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n}});\n"
            ),
        ),
        (
            "test-arrow",
            test_with(
                "  const send = jest.spyOn(mailer, \"send\");\n  [7].forEach((n) => announceMail(n));\n  expect(send).toHaveBeenCalledWith(\"order 7\");",
            ),
        ),
        (
            "test-other-method",
            test_with(
                "  const send = jest.spyOn(mailer, \"other\");\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");",
            ),
        ),
        (
            "test-chained-implementation",
            test_with(
                "  const send = jest.spyOn(mailer, \"send\").mockImplementation((m) => m);\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");",
            ),
        ),
        (
            "test-other-module",
            "import { announceMail } from \"../src/mail\";\nimport { mailer } from \"../src/other\";\ntest(\"announceMail\", () => {\n  const send = jest.spyOn(mailer, \"send\");\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n});\n"
                .to_string(),
        ),
        (
            // `spyOn` on a method that is already a mock returns that mock
            // with the calls the hook left on it.
            "file-hook",
            format!(
                "{IMPORT}beforeEach(() => {{\n  mailer.send = jest.fn();\n  mailer.send(\"order 7\");\n}});\n{SPY_TEST}"
            ),
        ),
        (
            "file-earlier-test",
            format!(
                "{IMPORT}\ntest(\"plain\", () => {{\n  announceMail(1);\n}});\n{SPY_TEST}"
            ),
        ),
        (
            "file-top-level-statement",
            format!("{IMPORT}mailer.send(\"order 7\");\n{SPY_TEST}"),
        ),
        (
            "file-describe",
            format!("{IMPORT}describe(\"mail\", () => {{\n{SPY_TEST}}});\n"),
        ),
        (
            "file-side-effect-import",
            format!("{IMPORT}import \"../src/other\";\n{SPY_TEST}"),
        ),
        (
            "file-empty-import",
            format!("{IMPORT}import {{}} from \"../src/other\";\n{SPY_TEST}"),
        ),
        (
            "file-type-import",
            format!("{IMPORT}import {{ type X }} from \"../src/other\";\n{SPY_TEST}"),
        ),
        (
            "file-helper-import",
            format!("{IMPORT}import {{ mailer as other }} from \"../src/other\";\n{SPY_TEST}"),
        ),
        (
            "file-timeout-word",
            format!("{IMPORT}{}", SPY_TEST.replace("\n});\n", "\n}, eval(\"1\"));\n")),
        ),
        (
            "file-second-spy",
            format!(
                "{IMPORT}\ntest(\"first\", () => {{\n  const send = jest.spyOn(mailer, \"send\");\n  announceMail(6);\n}});\n{SPY_TEST}"
            ),
        ),
        (
            "file-commented-spy",
            format!("{IMPORT}// jest.spyOn(mailer, \"other\")\n{SPY_TEST}"),
        ),
        (
            "file-escaped-spy",
            format!(
                "{IMPORT}\ntest(\"first\", () => {{\n  jest.\\u0073pyOn(mailer, \"send\");\n  announceMail(7);\n}});\n{SPY_TEST}"
            ),
        ),
        (
            "file-dynamic-import",
            format!(
                "{IMPORT}\ntest(\"first\", async () => {{\n  await import(\"../src/mail\");\n}});\n{SPY_TEST}"
            ),
        ),
        (
            "file-each",
            format!(
                "{IMPORT}\ntest.each([1])(\"x %d\", () => {{\n  const send = jest.spyOn(mailer, \"send\");\n  announceMail(7);\n  expect(send).toHaveBeenCalledWith(\"order 7\");\n}});\n"
            ),
        ),
        (
            "file-concurrent",
            format!(
                "{IMPORT}\ntest.concurrent(\"x\", () => {{\n  announceMail(1);\n}});\n{SPY_TEST}"
            ),
        ),
        (
            "file-module-mock",
            format!("{IMPORT}jest.mock(\"../src/other\");\n{SPY_TEST}"),
        ),
        (
            "file-retry",
            format!("{IMPORT}jest.retryTimes(2);\n{SPY_TEST}"),
        ),
    ];
    cases.extend(
        refused_tests
            .iter()
            .map(|(label, test)| (*label, OWNER, LINE, test.clone())),
    );
    // The changed call sits in an arrow the owner never runs.
    let arrow_owner = OWNER.replace(
        "  mailer.send(\"order \" + id);",
        "  const later = () => mailer.send(\"order \" + id);",
    );
    cases.push((
        "owner-call-in-arrow",
        &arrow_owner,
        (8, "  const later = () => mailer.send(\"order \" + id);"),
        format!("{IMPORT}{SPY_TEST}"),
    ));
    assert_all(&cases, ExposureClass::WeaklyExposed)
}
