//! #10522: package-base extends disclosure regression and config controls.
//! Config/import bytes reuse the native proof. `while` bodies remain dropped
//! after #10456 added extraction of `if`-guarded registrations.

use super::*;

const PACKAGE_JSON: &str = r#"{
  "name": "review-fixture",
  "version": "1.0.0",
  "private": true,
  "scripts": {
    "test": "vitest run"
  }
}
"#;

const PRICING: &str = r#"export default function tier(amount: number): string {
  if (amount >= 150) {
    return 'premium';
  }
  return 'standard';
}
"#;

const GLOBALS: &str = r#"declare const process: { env: { CI?: string } };
declare function require(name: string): any;
declare function it(name: string, body: () => void): void;
declare function expect(value: any): { toBe(expected: any): void };
"#;

const RETAINED_TEST: &str = r#"import tier from '../src/pricing';
it('boundary', () => {
  expect(tier(150)).toBe('premium');
  expect(tier(149)).toBe('standard');
});
"#;

const PACKAGE_BASE_MANIFEST: &str = r#"{"name":"@review/config","version":"1.0.0","private":true}
"#;

const PACKAGE_BASE_CONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "moduleResolution": "node",
    "strict": true,
    "noEmit": true,
    "paths": {
      "subject": [
        "../../../src/pricing.ts"
      ]
    }
  }
}
"#;

const PACKAGE_EXTENDS: &str = r#"{
  "extends": "@review/config/tsconfig.json",
  "include": [
    "src/**/*.ts",
    "tests/**/*.ts"
  ]
}
"#;

const ESCAPED_EXTENDS: &str = r#"{
  "\u0065xtends": "@review/config/tsconfig.json",
  "include": ["src/**/*.ts", "tests/**/*.ts"]
}
"#;

const ARRAY_EXTENDS: &str = r#"{
  "extends": ["@review/config/tsconfig.json"],
  "include": ["src/**/*.ts", "tests/**/*.ts"]
}
"#;

const COMMENTED_EXTENDS: &str = r#"{
  "extends" /* JSONC */ : "@review/config/tsconfig.json",
  "include": ["src/**/*.ts", "tests/**/*.ts"],
}
"#;

const ESCAPED_PATHS: &str = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "moduleResolution": "node",
    "strict": true,
    "noEmit": true,
    "\u0070aths": {"subject": ["./src/pricing.ts"]}
  },
  "include": ["src/**/*.ts", "tests/**/*.ts"]
}
"#;

const PLAIN_CONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "commonjs",
    "moduleResolution": "node",
    "strict": true,
    "noEmit": true
  },
  "include": [
    "src/**/*.ts",
    "tests/**/*.ts"
  ]
}
"#;

const RELATIVE_EXTENDS: &str = r#"{
  "extends": "./node_modules/@review/config/tsconfig.json",
  "include": [
    "src/**/*.ts",
    "tests/**/*.ts"
  ]
}
"#;

const ALIAS_GAP: &str = r#"import m from 'subject';
while (process.env.CI) {
  it(`boundary ${1}`, () => {
    expect(m(150)).toBe('premium');
  });
}
"#;

#[test]
fn package_base_paths_keep_extraction_disclosure_without_changing_findings() -> Result<(), String> {
    let mut reference_findings = None;
    for (label, package_base, config, gap_source, expected_partial) in [
        ("package_alias", true, PACKAGE_EXTENDS, ALIAS_GAP, true),
        ("escaped_extends", true, ESCAPED_EXTENDS, ALIAS_GAP, true),
        ("array_extends", true, ARRAY_EXTENDS, ALIAS_GAP, true),
        (
            "commented_extends",
            true,
            COMMENTED_EXTENDS,
            ALIAS_GAP,
            true,
        ),
        ("escaped_paths", false, ESCAPED_PATHS, ALIAS_GAP, true),
        (
            "package_literal",
            true,
            PACKAGE_EXTENDS,
            r#"import m from '../src/pricing';
while (process.env.CI) {
  it(`boundary ${1}`, () => {
    expect(m(150)).toBe('premium');
  });
}
"#,
            true,
        ),
        (
            "plain_literal",
            false,
            PLAIN_CONFIG,
            r#"import m from '../src/pricing';
while (process.env.CI) {
  it(`boundary ${1}`, () => {
    expect(m(150)).toBe('premium');
  });
}
"#,
            true,
        ),
        (
            "concat_index",
            false,
            PLAIN_CONFIG,
            r#"const selected = 'pricing';
const m = require('../src/' /* c */ + selected).default;
while (process.env.CI) {
  it(`boundary ${1}`, () => {
    expect(m(150)).toBe('premium');
  });
}
"#,
            true,
        ),
        (
            "unrelated_index",
            false,
            PLAIN_CONFIG,
            r#"const m = require('../src/');
while (process.env.CI) {
  it(`boundary ${1}`, () => {
    expect(m.unrelated).toBe(1);
  });
}
"#,
            false,
        ),
        (
            "relative_extends_alias",
            true,
            RELATIVE_EXTENDS,
            ALIAS_GAP,
            true,
        ),
    ] {
        let root = ts_unique_tempdir(label)?;
        for (path, source) in [
            (".gitignore", "node_modules/\n.ripr/\n"),
            ("package.json", PACKAGE_JSON),
            ("src/pricing.ts", PRICING),
            ("src/index.ts", "export const unrelated = 1;\n"),
            ("tests/globals.d.ts", GLOBALS),
            ("tests/pricing.test.ts", RETAINED_TEST),
            ("tsconfig.json", config),
            ("tests/gap.test.ts", gap_source),
        ] {
            ts_write_file(&root.join(path), source)?;
        }
        if package_base {
            ts_write_file(
                &root.join("node_modules/@review/config/package.json"),
                PACKAGE_BASE_MANIFEST,
            )?;
            ts_write_file(
                &root.join("node_modules/@review/config/tsconfig.json"),
                PACKAGE_BASE_CONFIG,
            )?;
        }
        assert!(
            !gap_source.contains("tier"),
            "{label}: avoid owner-name fallback"
        );
        let mut options = ts_analysis_options(root.clone());
        options.resolve_tsconfig_paths = true;
        let result = TypeScriptAdapter.analyze_diff(
            &options,
            &OraclePolicy::default(),
            &[changed_with_lines(
                "src/pricing.ts",
                &[(2, "  if (amount >= 150) {")],
            )],
        );
        let _ = std::fs::remove_dir_all(&root);
        let result = result?;
        assert_eq!(result.findings.len(), 1, "{label}: nonempty subject");
        assert_eq!(result.findings[0].class, ExposureClass::Exposed, "{label}");
        let partial = result.limitations.iter().find(|limitation| {
            limitation
                .bounded_detail
                .as_deref()
                .is_some_and(|detail| detail.contains("typescript_test_extraction_partial"))
        });
        assert_eq!(
            partial.is_some(),
            expected_partial,
            "{label}: extraction disclosure"
        );
        if let Some(partial) = partial {
            assert_eq!(
                partial.path.as_deref(),
                Some("tests/gap.test.ts"),
                "{label}"
            );
            assert_eq!(partial.affected_items, Some(1), "{label}");
            assert_eq!(
                result.limitations.len(),
                1,
                "{label}: only expected disclosure"
            );
        } else {
            assert!(
                result.limitations.is_empty(),
                "{label}: unrelated index remains complete"
            );
        }
        if let Some(reference) = &reference_findings {
            assert_eq!(
                &result.findings, reference,
                "{label}: findings and classification stay unchanged"
            );
        } else {
            reference_findings = Some(result.findings);
        }
    }
    Ok(())
}
