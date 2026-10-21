//! Instance methods reached only through other methods of the same class
//! (#8567, tj/commander.js): a test that calls the public method of an
//! instance it built ran the private helper at runtime, but the helper was
//! reported `no_static_path`. The reach is real but indirect, so it reads
//! `weakly_exposed` at most, and every shape that could rebind `this` or the
//! callee records no reach.

use super::*;

const CHANGED: &str = "    previous.push(value);";

const COMMAND: &str = "export class Command {\n  items: string[] = [];\n\n  _collect(value: string, previous: string[]): string[] {\n    previous.push(value);\n    return previous;\n  }\n\n  add(value: string): this {\n    this.items = this._collect(value, this.items);\n    return this;\n  }\n}\n";

const ADD_TEST: &str = "import { Command } from '../src/cmd';\n\ndescribe('add', () => {\n  it('collects in order', () => {\n    const program = new Command();\n    program.add('a').add('b');\n    expect(program.items).toEqual(['a', 'b']);\n  });\n});\n";

fn collect_finding(label: &str, owner_source: &str, test_source: &str) -> Result<Finding, String> {
    let line = owner_source
        .lines()
        .position(|line| line == CHANGED)
        .map(|index| index + 1)
        .ok_or_else(|| format!("{label}: the changed line is missing from the owner"))?;
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cmd.ts"), owner_source)?;
    ts_write_file(&root.join("tests/cmd.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cmd.ts", &[(line, CHANGED)])],
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
                .is_some_and(|owner| owner.0.ends_with("_collect"))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `_collect`"))
}

fn relates_through_entry(finding: &Finding) -> bool {
    finding
        .related_tests
        .iter()
        .any(|test| test.relation_reason == Some(crate::domain::RelationReason::HelperOwnerCall))
}

fn assert_entry_related(label: &str, owner_source: &str, test_source: &str) -> Result<(), String> {
    let finding = collect_finding(label, owner_source, test_source)?;
    assert!(
        relates_through_entry(&finding),
        "{label}: the test calls a method that reaches `_collect`, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{label}");
    // Indirect reach never reaches `exposed` on its own.
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{label}");
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("only through same-class callers")),
        "{label}: missing: {:?}",
        finding.missing
    );
    Ok(())
}

fn assert_no_static_path(label: &str, owner_source: &str, test_source: &str) -> Result<(), String> {
    let finding = collect_finding(label, owner_source, test_source)?;
    assert!(
        !relates_through_entry(&finding),
        "{label}: no method the test calls reaches `_collect`, related: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    Ok(())
}

fn entry_names(owner_source: &str, owner: &str) -> Vec<String> {
    extract_owners(Path::new("src/cmd.ts"), owner_source)
        .into_iter()
        .find(|candidate| candidate.name == owner)
        .map(|candidate| {
            candidate
                .module_entries
                .into_iter()
                .map(|entry| entry.name)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn public_method_call_relates_to_the_private_helper() -> Result<(), String> {
    assert_entry_related("one-hop", COMMAND, ADD_TEST)?;
    let finding = collect_finding("one-hop-summary", COMMAND, ADD_TEST)?;
    assert!(
        finding.missing.iter().any(|line| line.contains("(`add`)")),
        "the summary names the method the test calls: {:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn inline_new_instance_call_relates() -> Result<(), String> {
    let test = "import { Command } from '../src/cmd';\n\nit('collects', () => {\n  expect(new Command().add('a').items).toEqual(['a']);\n});\n";
    assert_entry_related("inline-new", COMMAND, test)
}

#[test]
fn two_hop_chain_relates() -> Result<(), String> {
    let source = COMMAND.replace(
        "    this.items = this._collect(value, this.items);\n    return this;",
        "    return this._apply(value);\n  }\n\n  _apply(value: string): this {\n    this.items = this._collect(value, this.items);\n    return this;",
    );
    assert_eq!(entry_names(&source, "_collect"), ["_apply", "add"]);
    assert_entry_related("two-hop", &source, ADD_TEST)
}

#[test]
fn constructor_that_calls_the_helper_relates() -> Result<(), String> {
    let source = COMMAND.replace(
        "  add(value: string): this {",
        "  constructor() {\n    this._collect('seed', this.items);\n  }\n\n  add(value: string): this {",
    );
    let test = "import { Command } from '../src/cmd';\n\nit('seeds', () => {\n  const program = new Command();\n  expect(program.items).toEqual(['seed']);\n});\n";
    assert_entry_related("constructor", &source, test)
}

#[test]
fn the_chain_stops_after_three_hops() -> Result<(), String> {
    let chain = "export class Command {\n  items: string[] = [];\n\n  _collect(value: string, previous: string[]): string[] {\n    previous.push(value);\n    return previous;\n  }\n\n  _a(value: string): string[] {\n    return this._collect(value, this.items);\n  }\n\n  _b(value: string): string[] {\n    return this._a(value);\n  }\n\n  _c(value: string): string[] {\n    return this._b(value);\n  }\n\n  add(value: string): string[] {\n    return this._c(value);\n  }\n}\n";
    // `add` is four calls away from `_collect`.
    assert_eq!(entry_names(chain, "_collect"), ["_a", "_b", "_c"]);
    assert_no_static_path("four-hops", chain, ADD_TEST)
}

#[test]
fn a_test_of_an_unrelated_method_does_not_relate() -> Result<(), String> {
    let source = COMMAND.replace(
        "  add(value: string): this {",
        "  reset(): this {\n    this.items = [];\n    return this;\n  }\n\n  add(value: string): this {",
    );
    let test = "import { Command } from '../src/cmd';\n\nit('resets', () => {\n  const program = new Command();\n  program.reset();\n  expect(program.items).toEqual([]);\n});\n";
    assert_no_static_path("unrelated-method", &source, test)
}

#[test]
fn a_method_called_on_another_class_does_not_relate() -> Result<(), String> {
    let test = "import { Option } from '../src/option';\n\nit('collects', () => {\n  const option = new Option();\n  option.add('a');\n  expect(option.items).toEqual(['a']);\n});\n";
    assert_no_static_path("other-class", COMMAND, test)
}

#[test]
fn a_nested_function_may_rebind_this_and_records_no_edge() -> Result<(), String> {
    let source = COMMAND.replace(
        "    this.items = this._collect(value, this.items);",
        "    [value].forEach(function (item) {\n      this.items = this._collect(item, this.items);\n    });",
    );
    assert!(entry_names(&source, "_collect").is_empty());
    assert_no_static_path("nested-function", &source, ADD_TEST)
}

#[test]
fn an_arrow_callback_keeps_this_and_records_the_edge() -> Result<(), String> {
    let source = COMMAND.replace(
        "    this.items = this._collect(value, this.items);",
        "    [value].forEach((item) => {\n      this.items = this._collect(item, this.items);\n    });",
    );
    assert_eq!(entry_names(&source, "_collect"), ["add"]);
    assert_entry_related("arrow-callback", &source, ADD_TEST)
}

#[test]
fn an_instance_assignment_shadows_the_method() -> Result<(), String> {
    let assigned = COMMAND.replace(
        "  add(value: string): this {",
        "  hook(fn: (value: string, previous: string[]) => string[]): void {\n    this._collect = fn;\n  }\n\n  add(value: string): this {",
    );
    assert!(entry_names(&assigned, "_collect").is_empty());
    assert_no_static_path("assigned", &assigned, ADD_TEST)?;
    let defined = COMMAND.replace(
        "  add(value: string): this {",
        "  seal(): void {\n    Object.defineProperty(this, '_collect', { value: () => [] });\n  }\n\n  add(value: string): this {",
    );
    assert!(entry_names(&defined, "_collect").is_empty());
    // A comparison is not an assignment.
    let compared = COMMAND.replace(
        "  add(value: string): this {",
        "  same(fn: unknown): boolean {\n    return this._collect === fn;\n  }\n\n  add(value: string): this {",
    );
    assert_eq!(entry_names(&compared, "_collect"), ["add"]);
    Ok(())
}

#[test]
fn mentions_that_are_not_this_calls_record_no_edge() {
    for (label, call) in [
        (
            "callback reference",
            "this.items = [value].map(this._collect);",
        ),
        (
            "other receiver",
            "this.items = other._collect(value, this.items);",
        ),
        ("line comment", "// this._collect(value, this.items)"),
        ("block comment", "/* this._collect(value, this.items) */"),
        ("string", "const note = 'this._collect(value)';"),
        (
            "private name",
            "this.items = this.#_collect(value, this.items);",
        ),
        (
            "longer name",
            "this.items = this._collectAll(value, this.items);",
        ),
    ] {
        let source = COMMAND.replace(
            "    this.items = this._collect(value, this.items);",
            &format!("    {call}"),
        );
        assert!(
            entry_names(&source, "_collect").is_empty(),
            "{label}: {call}"
        );
    }
}

#[test]
fn an_overloaded_or_duplicated_name_is_not_a_node() {
    let duplicated = COMMAND.replace(
        "  add(value: string): this {",
        "  _collect(value: number): number {\n    return value;\n  }\n\n  add(value: string): this {",
    );
    assert!(entry_names(&duplicated, "_collect").is_empty());
    // A caller named twice is no entry either.
    let twice = COMMAND.replace(
        "  add(value: string): this {",
        "  add(value: number): number {\n    return value;\n  }\n\n  add(value: string): this {",
    );
    assert!(entry_names(&twice, "_collect").is_empty());
}

#[test]
fn static_and_setter_owners_record_no_entries() {
    let source = "export class Command {\n  static _make(): Command {\n    return new Command();\n  }\n\n  set value(v: string) {\n    this._store(v);\n  }\n\n  _store(v: string): void {\n    this.v = v;\n  }\n\n  build(): Command {\n    return Command._make();\n  }\n}\n";
    assert!(entry_names(source, "_make").is_empty());
    // A setter reaches `_store` by assignment, which is not a call needle.
    assert!(entry_names(source, "_store").is_empty());
}

#[test]
fn a_direct_call_of_the_helper_wins_over_the_entry() -> Result<(), String> {
    let test = "import { Command } from '../src/cmd';\n\nit('collects', () => {\n  const program = new Command();\n  expect(program._collect('a', [])).toEqual(['a']);\n  program.add('b');\n});\n";
    let finding = collect_finding("direct", COMMAND, test)?;
    assert!(
        !relates_through_entry(&finding),
        "a direct receiver call is the tier-1 relation: {:?}",
        finding.related_tests
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes);
    Ok(())
}

#[test]
fn a_mocked_owner_module_withholds_the_entry() -> Result<(), String> {
    let test = "import { Command } from '../src/cmd';\n\njest.mock('../src/cmd');\n\nit('collects', () => {\n  const program = new Command();\n  program.add('a');\n  expect(program.items).toEqual(['a']);\n});\n";
    let finding = collect_finding("mocked", COMMAND, test)?;
    assert!(
        !relates_through_entry(&finding),
        "{:?}",
        finding.related_tests
    );
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}
