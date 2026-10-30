//! Getter owners reached through an exported factory (#10402, colinhacks/zod):
//! a test that binds `const box = cached(...)` and reads `box.value` ran the
//! getter of the class the factory returned, but the getter was reported
//! `no_static_path` because reach was keyed on a call of the owner name. The
//! relation lends reach and the assertion's oracle and the classifier decides
//! the class as for a constructed receiver; every shape that could make the
//! receiver or the getter something else records no reach.

use super::*;

const CHANGED: &str = "    if (getter !== undefined) {";

const CACHE: &str = "export class Cached<T> {\n  private _getter: (() => T) | undefined;\n  private _value: T | undefined;\n\n  constructor(getter: () => T) {\n    this._getter = getter;\n  }\n\n  get value(): T {\n    const getter = this._getter;\n    if (getter !== undefined) {\n      this._value = getter();\n      this._getter = undefined;\n    }\n    return this._value as T;\n  }\n}\n\nexport function cached<T>(getter: () => T): Cached<T> {\n  return new Cached(getter);\n}\n";

const READ_TEST: &str = "import { cached } from '../src/cache';\n\ntest('memoizes', () => {\n  let runs = 0;\n  const box = cached(() => ++runs);\n  expect(runs).toBe(0);\n  expect(box.value).toBe(1);\n  expect(box.value).toBe(1);\n  expect(runs).toBe(1);\n});\n";

fn getter_finding(label: &str, owner_source: &str, test_source: &str) -> Result<Finding, String> {
    getter_finding_at(label, owner_source, test_source, CHANGED)
}

fn getter_finding_at(
    label: &str,
    owner_source: &str,
    test_source: &str,
    changed: &str,
) -> Result<Finding, String> {
    let line = owner_source
        .lines()
        .position(|line| line == changed)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cache.ts"), owner_source)?;
    ts_write_file(&root.join("tests/cache.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cache.ts", &[(line, changed)])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("::value"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for the `value` getter"))
}

fn assert_related(label: &str, owner_source: &str, test_source: &str) -> Result<(), String> {
    let finding = getter_finding(label, owner_source, test_source)?;
    assert!(
        !finding.related_tests.is_empty(),
        "{label}: the test reads the getter of a factory-built instance"
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{label}");
    // The changed predicate boundary is not discriminated by this test, so the
    // finding stays below `exposed`. The class for other changed lines is the
    // classifier's, exactly as for a constructed receiver (see
    // `factory_receiver_classifies_like_a_constructed_receiver`).
    assert_ne!(finding.class, ExposureClass::Exposed, "{label}");
    assert_ne!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

fn assert_no_static_path(label: &str, owner_source: &str, test_source: &str) -> Result<(), String> {
    let finding = getter_finding(label, owner_source, test_source)?;
    assert!(
        finding.related_tests.is_empty(),
        "{label}: no reach is established, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

fn factory_entry_names(owner_source: &str) -> Vec<String> {
    extract_owners(Path::new("src/cache.ts"), owner_source)
        .into_iter()
        .find(|candidate| candidate.name == "value")
        .map(|candidate| {
            candidate
                .module_entries
                .into_iter()
                .filter(|entry| entry.instance_factory)
                .map(|entry| entry.name)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn getter_read_on_a_factory_built_receiver_relates() -> Result<(), String> {
    assert_eq!(factory_entry_names(CACHE), ["cached"]);
    assert_related("factory-read", CACHE, READ_TEST)
}

#[test]
fn namespace_import_factory_call_relates() -> Result<(), String> {
    let test = READ_TEST
        .replace("import { cached }", "import * as util")
        .replace("= cached(", "= util.cached(");
    assert_related("factory-namespace", CACHE, &test)
}

#[test]
fn renamed_import_factory_call_relates() -> Result<(), String> {
    let test = READ_TEST
        .replace("{ cached }", "{ cached as memo }")
        .replace("= cached(", "= memo(");
    assert_related("factory-renamed", CACHE, &test)
}

#[test]
fn arrow_const_factory_and_export_list_relate() -> Result<(), String> {
    let source = CACHE.replace(
        "export function cached<T>(getter: () => T): Cached<T> {\n  return new Cached(getter);\n}\n",
        "const make = <T>(getter: () => T) => new Cached(getter);\nexport { make as cached };\n",
    );
    assert_eq!(factory_entry_names(&source), ["cached"]);
    assert_related("factory-arrow", &source, READ_TEST)
}

#[test]
fn factory_without_a_getter_read_is_no_static_path() -> Result<(), String> {
    let test = "import { cached } from '../src/cache';\n\ntest('runs', () => {\n  let runs = 0;\n  const box = cached(() => ++runs);\n  expect(box).toBeDefined();\n});\n";
    assert_no_static_path("factory-no-read", CACHE, test)
}

#[test]
fn getter_read_on_an_unrelated_receiver_is_no_static_path() -> Result<(), String> {
    let test = "import { cached } from '../src/cache';\n\ntest('other', () => {\n  const box = { value: 1 };\n  const unused = cached(() => 2);\n  expect(box.value).toBe(1);\n});\n";
    assert_no_static_path("unrelated-receiver", CACHE, test)
}

#[test]
fn rebound_receiver_is_no_static_path() -> Result<(), String> {
    let test = READ_TEST.replace(
        "const box = cached(() => ++runs);",
        "let box = cached(() => ++runs);\n  box = { value: 1 } as never;",
    );
    assert_no_static_path("rebound-receiver", CACHE, &test)
}

#[test]
fn redeclared_receiver_is_no_static_path() -> Result<(), String> {
    let test = READ_TEST.replace(
        "  expect(runs).toBe(0);",
        "  {\n    const box = { value: 1 };\n    expect(box.value).toBe(1);\n  }",
    );
    assert_no_static_path("redeclared-receiver", CACHE, &test)
}

#[test]
fn result_of_the_factory_call_is_not_the_receiver() -> Result<(), String> {
    let test = READ_TEST.replace("cached(() => ++runs);", "cached(() => ++runs).value;");
    assert_no_static_path("chained-read", CACHE, &test)
}

#[test]
fn computed_access_is_no_static_path() -> Result<(), String> {
    let test = READ_TEST.replace("box.value", "box['value']");
    assert_no_static_path("computed-access", CACHE, &test)
}

#[test]
fn getter_spy_is_no_static_path() -> Result<(), String> {
    let test = READ_TEST.replace(
        "  expect(runs).toBe(0);",
        "  vi.spyOn(box, 'value', 'get').mockReturnValue(1);",
    );
    assert_no_static_path("getter-spy", CACHE, &test)
}

#[test]
fn defined_property_on_the_receiver_is_no_static_path() -> Result<(), String> {
    let test = READ_TEST.replace(
        "  expect(runs).toBe(0);",
        "  Object.defineProperty(box, 'value', { value: 1 });",
    );
    assert_no_static_path("define-property", CACHE, &test)
}

#[test]
fn mocked_owner_module_is_no_static_path() -> Result<(), String> {
    let test = format!("vi.mock('../src/cache');\n{READ_TEST}");
    assert_no_static_path("mocked-module", CACHE, &test)
}

#[test]
fn mocked_factory_is_no_static_path() -> Result<(), String> {
    let test = READ_TEST.replace(
        "  let runs = 0;",
        "  vi.spyOn(cacheModule, 'cached').mockReturnValue({ value: 1 } as never);\n  let runs = 0;",
    );
    let test = format!("import * as cacheModule from '../src/cache';\n{test}");
    assert_no_static_path("mocked-factory", CACHE, &test)
}

#[test]
fn shadowed_factory_name_is_no_static_path() -> Result<(), String> {
    let test = READ_TEST.replace(
        "  let runs = 0;",
        "  const cached = (getter: () => number) => ({ value: getter() });\n  let runs = 0;",
    );
    assert_no_static_path("shadowed-factory", CACHE, &test)
}

#[test]
fn factory_from_an_unrelated_module_is_no_static_path() -> Result<(), String> {
    let test = READ_TEST.replace("'../src/cache'", "'../src/other'");
    assert_no_static_path("unrelated-module", CACHE, &test)
}

#[test]
fn factories_that_may_build_something_else_record_no_entry() {
    let subclass = CACHE.replace("  return new Cached(getter);", "  return new Sub(getter);")
        + "class Sub<T> extends Cached<T> {\n  get value(): T {\n    return undefined as T;\n  }\n}\n";
    assert!(factory_entry_names(&subclass).is_empty(), "other class");
    let conditional = CACHE.replace(
        "  return new Cached(getter);",
        "  if (getter.length > 0) {\n    return new Cached(getter);\n  }\n  return { value: 1 } as never;",
    );
    assert!(
        factory_entry_names(&conditional).is_empty(),
        "mixed returns"
    );
    let falls_through = CACHE.replace(
        "  return new Cached(getter);",
        "  if (getter.length > 0) {\n    return new Cached(getter);\n  }",
    );
    assert!(
        factory_entry_names(&falls_through).is_empty(),
        "no final return"
    );
    let asynchronous = CACHE.replace("export function cached", "export async function cached");
    assert!(factory_entry_names(&asynchronous).is_empty(), "async");
    let shadowed_class = CACHE.replace(
        "  return new Cached(getter);",
        "  const Cached = class {};\n  return new Cached(getter);",
    );
    assert!(
        factory_entry_names(&shadowed_class).is_empty(),
        "shadowed class"
    );
    let param_class = CACHE.replace("(getter: () => T)", "(getter: () => T, Cached = Object)");
    assert!(factory_entry_names(&param_class).is_empty(), "param class");
    let reassigned = format!("{CACHE}cached = undefined as never;\n");
    assert!(factory_entry_names(&reassigned).is_empty(), "reassigned");
    let duplicate = format!("{CACHE}export const cached = 1;\n");
    assert!(factory_entry_names(&duplicate).is_empty(), "duplicate");
    let unexported = CACHE.replace("export function cached", "function cached");
    assert!(factory_entry_names(&unexported).is_empty(), "not exported");
}

#[test]
fn classes_that_can_replace_the_getter_record_no_entry() {
    let field = CACHE.replace(
        "  private _value: T | undefined;",
        "  private _value: T | undefined;\n  value = 1;",
    );
    assert!(factory_entry_names(&field).is_empty(), "instance field");
    let assigned = CACHE.replace(
        "    this._getter = getter;\n  }",
        "    this._getter = getter;\n    this.value = 1;\n  }",
    );
    assert!(factory_entry_names(&assigned).is_empty(), "this assignment");
    let prototype = format!("{CACHE}Cached.prototype.extra = 1;\n");
    assert!(factory_entry_names(&prototype).is_empty(), "prototype edit");
    let define =
        format!("{CACHE}Object.defineProperty(Cached.prototype, 'value', {{ value: 1 }});\n");
    assert!(factory_entry_names(&define).is_empty(), "defineProperty");
    let repeated =
        format!("{CACHE}function scope() {{\n  class Cached {{}}\n  return Cached;\n}}\n");
    assert!(factory_entry_names(&repeated).is_empty(), "repeated class");
}

#[test]
fn ordinary_methods_record_no_factory_entry() {
    let source = CACHE.replace("get value(): T", "value(): T");
    assert!(factory_entry_names(&source).is_empty());
}

#[test]
fn factory_receiver_classifies_like_a_constructed_receiver() -> Result<(), String> {
    // The relation lends reach and the assertion's oracle; the classifier then
    // decides the class as for `new Cached(...)`, whatever line changed.
    let returned = "    return this._value as T;";
    let direct = READ_TEST
        .replace("import { cached }", "import { Cached }")
        .replace("= cached(", "= new Cached(");
    let through_factory = getter_finding_at("parity-factory", CACHE, READ_TEST, returned)?;
    let constructed = getter_finding_at("parity-new", CACHE, &direct, returned)?;
    assert!(!through_factory.related_tests.is_empty());
    assert_eq!(through_factory.class, constructed.class);
    assert_eq!(through_factory.ripr.reach.state, StageState::Yes);
    Ok(())
}

fn with_namespace_import(test: &str) -> String {
    test.replace("import { cached }", "import * as m")
        .replace("= cached(", "= m.cached(")
}

#[test]
fn once_variants_and_mocked_wrappers_on_the_factory_are_no_static_path() -> Result<(), String> {
    for (label, spy) in [
        (
            "return-once",
            "vi.spyOn(m, 'cached').mockReturnValueOnce({ value: 1 } as never);",
        ),
        (
            "implementation-once",
            "vi.spyOn(m, 'cached').mockImplementationOnce(() => ({ value: 1 }) as never);",
        ),
        ("call-through-spy", "vi.spyOn(m, \"cached\");"),
        (
            "mocked-wrapper",
            "vi.mocked(m.cached).mockReturnValueOnce({ value: 1 } as never);",
        ),
        ("assigned", "m.cached = (() => ({ value: 1 })) as never;"),
    ] {
        let test = with_namespace_import(READ_TEST)
            .replace("  let runs = 0;", &format!("  {spy}\n  let runs = 0;"));
        assert_no_static_path(label, CACHE, &test)?;
    }
    Ok(())
}

#[test]
fn receiver_identity_shapes_are_no_static_path() -> Result<(), String> {
    let imports = "import { cached } from '../src/cache';\n\n";
    for (label, body, prelude) in [
        (
            "block-scope-leak",
            "  {\n    const box = cached(() => 1);\n    expect(box).toBeDefined();\n  }\n  expect(box.value).toBe(1);",
            "const box = { value: 1 };\n",
        ),
        (
            "catch-parameter",
            "  const box = cached(() => 1);\n  try {\n    throw new Error('x');\n  } catch (box) {\n    expect(box.value).toBe(1);\n  }",
            "",
        ),
        (
            "arrow-parameter",
            "  const box = cached(() => 1);\n  [{ value: 1 }].forEach((box) => {\n    expect(box.value).toBe(1);\n  });",
            "",
        ),
        (
            "function-parameter",
            "  const box = cached(() => 1);\n  function check(box: { value: number }) {\n    expect(box.value).toBe(1);\n  }\n  check({ value: 1 });",
            "",
        ),
        (
            "oracle-on-another-object",
            "  const box = cached(() => 1);\n  void box.value;\n  const other = { value: 1 };\n  expect(other.value).toBe(1);",
            "",
        ),
        (
            "read-outside-expect",
            "  const box = cached(() => 1);\n  const seen = box.value;\n  expect(seen).toBe(1);",
            "",
        ),
    ] {
        let test = format!("{imports}{prelude}test('t', () => {{\n{body}\n}});\n");
        assert_no_static_path(label, CACHE, &test)?;
    }
    Ok(())
}

#[test]
fn file_level_replacements_are_no_static_path() -> Result<(), String> {
    for (label, prelude) in [
        (
            "top-level-define-property",
            "Object.defineProperty(Cached.prototype, 'value', { get: () => 1 });\n",
        ),
        (
            "before-each-getter-spy",
            "beforeEach(() => {\n  vi.spyOn(Cached.prototype, 'value', 'get').mockReturnValue(1);\n});\n",
        ),
        ("proto-write", "const base = { __proto__: null };\n"),
        ("prototype-assignment", "Cached.prototype.value = 1;\n"),
        (
            "top-level-factory-spy",
            "vi.spyOn(cacheModule, 'cached').mockReturnValueOnce({ value: 1 } as never);\n",
        ),
    ] {
        let test = format!("{prelude}{READ_TEST}");
        assert_no_static_path(label, CACHE, &test)?;
    }
    Ok(())
}

#[test]
fn classes_with_other_instance_sources_record_no_entry() {
    let returns_object = CACHE.replace(
        "    this._getter = getter;\n  }",
        "    this._getter = getter;\n    return {} as never;\n  }",
    );
    assert!(
        factory_entry_names(&returns_object).is_empty(),
        "constructor return"
    );
    let imported_base = format!(
        "import {{ Base }} from './base';\n{}",
        CACHE.replace(
            "export class Cached<T> {",
            "export class Cached<T> extends Base {"
        )
    );
    assert!(
        factory_entry_names(&imported_base).is_empty(),
        "imported base"
    );
    let call_base = CACHE.replace(
        "export class Cached<T> {",
        "export class Cached<T> extends mixin(Object) {",
    );
    assert!(factory_entry_names(&call_base).is_empty(), "call base");
    let clean_base = format!(
        "class Base {{}}\n{}",
        CACHE.replace(
            "export class Cached<T> {",
            "export class Cached<T> extends Base {"
        )
    );
    assert_eq!(factory_entry_names(&clean_base), ["cached"], "clean base");
    let base_field = format!(
        "class Base {{\n  value = 1;\n}}\n{}",
        CACHE.replace(
            "export class Cached<T> {",
            "export class Cached<T> extends Base {"
        )
    );
    assert!(factory_entry_names(&base_field).is_empty(), "base field");
    let base_returns = format!(
        "class Base {{\n  constructor() {{\n    return {{}};\n  }}\n}}\n{}",
        CACHE.replace(
            "export class Cached<T> {",
            "export class Cached<T> extends Base {"
        )
    );
    assert!(
        factory_entry_names(&base_returns).is_empty(),
        "base constructor return"
    );
    let reassigned_class = format!("{CACHE}Cached = class {{}} as never;\n");
    assert!(
        factory_entry_names(&reassigned_class).is_empty(),
        "class name reassigned"
    );
    let duplicate_getter = CACHE.replace(
        "  get value(): T {",
        "  get value(): T {\n    return this._value as T;\n  }\n\n  get value(): T {",
    );
    assert!(
        factory_entry_names(&duplicate_getter).is_empty(),
        "duplicate getter"
    );
    let proto = format!("{CACHE}const shape = {{ __proto__: null }};\n");
    assert!(factory_entry_names(&proto).is_empty(), "__proto__");
}
