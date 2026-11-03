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
    // Indirect reach never reaches `exposed` on its own, and an exact oracle
    // behind an entry is unknown, not an actionable gap (#9290).
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{label}");
    assert_eq!(
        finding.stop_reasons,
        [StopReason::PropagationEvidenceUnknown],
        "{label}"
    );
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
    // Deleting from an instance removes only an own property, so the
    // prototype method still runs.
    let instance_deleted = format!(
        "{HELPER_SPY_TEST_HEAD}  delete program._collect;\n  program.add('a');\n  expect(program.items).toEqual(['a']);\n}});\n"
    );
    assert_entry_related("instance-deleted", COMMAND, &instance_deleted)?;
    // A comparison is not an assignment.
    let compared = COMMAND.replace(
        "  add(value: string): this {",
        "  same(fn: unknown): boolean {\n    return this._collect === fn;\n  }\n\n  add(value: string): this {",
    );
    assert_eq!(entry_names(&compared, "_collect"), ["add"]);
    Ok(())
}

#[test]
fn method_reference_in_sync_callback_records_an_edge() {
    for (label, call) in [
        ("map callback", "this.items = [value].map(this._collect);"),
        (
            "forEach with thisArg",
            "this.items = items.forEach(this._collect, this);",
        ),
        (
            "bound map callback",
            "this.items = items.map(this._collect.bind(this));",
        ),
        (
            "call",
            "this.items = this._collect.call(this, value, this.items);",
        ),
        (
            "apply",
            "this.items = this._collect.apply(this, [value, this.items]);",
        ),
        (
            "bound immediate invoke",
            "this.items = this._collect.bind(this)(value, this.items);",
        ),
    ] {
        let source = COMMAND.replace(
            "    this.items = this._collect(value, this.items);",
            &format!("    {call}"),
        );
        assert_eq!(entry_names(&source, "_collect"), ["add"], "{label}: {call}");
    }
}

#[test]
fn method_reference_in_sync_callback_relates_weakly() -> Result<(), String> {
    let source = COMMAND.replace(
        "    this.items = this._collect(value, this.items);",
        "    this.items = [value].map(this._collect);",
    );
    assert_entry_related("map-callback-ref", &source, ADD_TEST)?;
    let finding = collect_finding("map-callback-ref-class", &source, ADD_TEST)?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn deferred_or_stored_method_references_record_no_edge() {
    for (label, call) in [
        ("deferred bind", "setTimeout(this._collect.bind(this));"),
        ("stored bind", "const f = this._collect.bind(this);"),
        ("push registration", "this.cbs.push(this._collect);"),
        ("event listener", "emitter.on('x', this._collect);"),
        (
            "not first map arg",
            "this.items = items.map(x, this._collect);",
        ),
        (
            "longer callee name",
            "this.items = items.map(this._collectAll);",
        ),
        ("private callee", "this.items = items.map(this.#_collect);"),
        (
            "other receiver call",
            "this.items = other._collect.call(this, value);",
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
fn deferred_bound_method_reference_does_not_relate() -> Result<(), String> {
    let source = COMMAND.replace(
        "    this.items = this._collect(value, this.items);",
        "    setTimeout(this._collect.bind(this));",
    );
    assert_no_static_path("deferred-bind", &source, ADD_TEST)
}

#[test]
fn mentions_that_are_not_this_calls_record_no_edge() {
    for (label, call) in [
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

const CALL_LINE: &str = "    this.items = this._collect(value, this.items);";

#[test]
fn an_instance_field_of_the_same_name_shadows_the_method() -> Result<(), String> {
    // `helper = () => ...` replaces the prototype method on every instance.
    let fielded = COMMAND.replace(
        "  add(value: string): this {",
        "  _collect = (value: string, previous: string[]): string[] => previous;\n\n  add(value: string): this {",
    );
    assert!(entry_names(&fielded, "_collect").is_empty());
    assert_no_static_path("instance-field", &fielded, ADD_TEST)?;
    // A computed field key may define any name.
    let computed = COMMAND.replace(
        "  add(value: string): this {",
        "  [Symbol.for('x')] = 1;\n\n  add(value: string): this {",
    );
    assert!(entry_names(&computed, "_collect").is_empty());
    // A static field is not an instance field.
    let static_field = COMMAND.replace(
        "  add(value: string): this {",
        "  static _collect = 1;\n\n  add(value: string): this {",
    );
    assert_eq!(entry_names(&static_field, "_collect"), ["add"]);
    Ok(())
}

#[test]
fn an_arrow_that_is_created_but_not_passed_to_a_call_records_no_edge() -> Result<(), String> {
    let unused = COMMAND.replace(
        CALL_LINE,
        "    const later = () => this._collect(value, this.items);",
    );
    assert!(entry_names(&unused, "_collect").is_empty());
    assert_no_static_path("unused-arrow", &unused, ADD_TEST)?;
    // The same arrow handed to a call is read as invoked.
    let passed = COMMAND.replace(
        CALL_LINE,
        "    [value].forEach((item) => this._collect(item, this.items));",
    );
    assert_eq!(entry_names(&passed, "_collect"), ["add"]);
    // An unused arrow does not hide a real call in the same method.
    let mixed = COMMAND.replace(
        CALL_LINE,
        "    const noop = () => 0;\n    this.items = this._collect(value, this.items);",
    );
    assert_eq!(entry_names(&mixed, "_collect"), ["add"]);
    Ok(())
}

#[test]
fn an_object_literal_method_binds_its_own_this() -> Result<(), String> {
    let nested = COMMAND.replace(
        CALL_LINE,
        "    const worker = { _collect() { return 2; }, run() { return this._collect(); } };\n    worker.run();",
    );
    assert!(entry_names(&nested, "_collect").is_empty());
    assert_no_static_path("object-method", &nested, ADD_TEST)
}

#[test]
fn keywords_in_strings_and_comments_do_not_hide_a_real_call() -> Result<(), String> {
    let literal = COMMAND.replace(
        CALL_LINE,
        "    const note = 'function class';\n    // function () {}\n    this.items = this._collect(value, this.items);",
    );
    assert_eq!(entry_names(&literal, "_collect"), ["add"]);
    assert_entry_related("literal-keywords", &literal, ADD_TEST)
}

#[test]
fn a_method_header_without_a_return_type_is_not_a_nested_method() {
    let untyped = COMMAND.replace("  add(value: string): this {", "  add(value) {");
    assert_eq!(entry_names(&untyped, "_collect"), ["add"]);
}

fn with_call(replacement: &str) -> String {
    COMMAND.replace(CALL_LINE, replacement)
}

#[test]
fn an_arrow_stored_rather_than_run_records_no_edge() {
    // Array element, logical operand and a stored callback are not run by the
    // method (#9942 item 2).
    for (label, line) in [
        (
            "array-element",
            "    const queue = [value, () => this._collect(value, this.items)];",
        ),
        (
            "logical-operand",
            "    const run = value || (() => this._collect(value, this.items));",
        ),
        (
            "stored-by-push",
            "    this.callbacks.push(() => this._collect(value, this.items));",
        ),
    ] {
        assert!(
            entry_names(&with_call(line), "_collect").is_empty(),
            "{label}"
        );
    }
}

#[test]
fn a_callback_a_callee_may_run_later_or_never_records_no_edge() {
    // Deferred or event-driven callees (#9942 item 3).
    for (label, line) in [
        (
            "timer",
            "    setTimeout(() => this._collect(value, this.items), 0);",
        ),
        (
            "listener",
            "    this.emitter.on('x', () => this._collect(value, this.items));",
        ),
        (
            "microtask",
            "    queueMicrotask(() => this._collect(value, this.items));",
        ),
        (
            "subscription",
            "    this.stream.subscribe(() => this._collect(value, this.items));",
        ),
        (
            "promise-then",
            "    this.ready.then(() => this._collect(value, this.items));",
        ),
    ] {
        assert!(
            entry_names(&with_call(line), "_collect").is_empty(),
            "{label}"
        );
    }
    // Synchronous array callbacks keep the edge, including a second argument
    // position and an optional call.
    for (label, line) in [
        (
            "map",
            "    [value].map((item) => this._collect(item, this.items));",
        ),
        (
            "reduce-first-argument",
            "    [value].reduce((acc, item) => this._collect(item, acc), this.items);",
        ),
        (
            "optional-call",
            "    this.list?.forEach((item) => this._collect(item, this.items));",
        ),
    ] {
        assert_eq!(
            entry_names(&with_call(line), "_collect"),
            ["add"],
            "{label}"
        );
    }
}

#[test]
fn a_generator_entry_does_not_run_its_body_until_iterated() {
    // `*items()` and `async *items()` only create an iterator (#9942 item 4).
    for (label, header) in [
        ("generator", "  *items(value: string) {"),
        ("async-generator", "  async *items(value: string) {"),
    ] {
        let source = COMMAND.replace(
            "  add(value: string): this {",
            &format!(
                "{header}\n    yield this._collect(value, this.items);\n  }}\n\n  add(value: string): this {{"
            ),
        );
        assert_eq!(entry_names(&source, "_collect"), ["add"], "{label}");
    }
    // A generator is not a callee either: `this.items()` does not run it.
    let called = COMMAND.replace(
        CALL_LINE,
        "    this.items = this.walk(value);",
    ).replace(
        "  add(value: string): this {",
        "  *walk(value: string) {\n    yield this._collect(value, this.items);\n  }\n\n  add(value: string): this {",
    );
    assert!(entry_names(&called, "_collect").is_empty());
}

#[test]
fn an_unused_function_expression_does_not_drop_a_real_call() -> Result<(), String> {
    // The method really calls the helper outside the function expression
    // (#9942 item 5).
    let source = with_call(
        "    const later = function () {\n      this._collect(value, this.items);\n    };\n    this.items = this._collect(value, this.items);",
    );
    assert_eq!(entry_names(&source, "_collect"), ["add"]);
    assert_entry_related("unused-function-expression", &source, ADD_TEST)?;
    // A call only inside the function expression is not an edge.
    let inside = with_call(
        "    const later = function () {\n      this._collect(value, this.items);\n    };",
    );
    assert!(entry_names(&inside, "_collect").is_empty());
    // A function declaration nested in the method is treated the same way.
    let declared = with_call(
        "    function inner() {\n      return this._collect(value, this.items);\n    }\n    this.items = this._collect(value, this.items);",
    );
    assert_eq!(entry_names(&declared, "_collect"), ["add"]);
    // A nested class still drops the caller entirely.
    let nested_class = with_call(
        "    const Local = class { run() { return 1; } };\n    this.items = this._collect(value, this.items);",
    );
    assert!(entry_names(&nested_class, "_collect").is_empty());
    // A return type annotation after the parameters cannot be told from a
    // body, so the caller is dropped.
    let annotated = with_call(
        "    const typed = function (item: string): { ok: boolean } {\n      return { ok: this._collect(item, []).length > 0 };\n    };\n    this.items = this._collect(value, this.items);",
    );
    assert!(entry_names(&annotated, "_collect").is_empty());
    Ok(())
}

const HELPER_SPY_TEST_HEAD: &str = "import { Command } from '../src/cmd';\n\nit('collects', () => {\n  const program = new Command();\n";

#[test]
fn a_test_that_replaces_the_helper_does_not_run_the_real_code() -> Result<(), String> {
    // Spy with a fabricated value, and a plain assignment on the instance
    // (#9942 item 1): the public method no longer reaches the real helper.
    for (label, setup) in [
        (
            "spy-mock-return",
            "  jest.spyOn(Command.prototype, '_collect').mockReturnValue(['z']);\n",
        ),
        (
            "spy-mock-implementation",
            "  jest.spyOn(program, '_collect').mockImplementation(() => []);\n",
        ),
        ("assigned", "  program._collect = jest.fn();\n"),
        (
            "prototype-assigned",
            "  Command.prototype._collect = jest.fn();\n",
        ),
        (
            "prototype-deleted",
            "  delete Command.prototype._collect;\n",
        ),
        (
            "define-property",
            "  Object.defineProperty(program, '_collect', { value: () => [] });\n",
        ),
    ] {
        let test = format!(
            "{HELPER_SPY_TEST_HEAD}{setup}  program.add('a');\n  expect(program.items).toEqual(['a']);\n}});\n"
        );
        assert_no_static_path(label, COMMAND, &test)?;
    }
    // A bare call-through spy still runs the real helper.
    let through = format!(
        "{HELPER_SPY_TEST_HEAD}  jest.spyOn(Command.prototype, '_collect');\n  program.add('a');\n  expect(program.items).toEqual(['a']);\n}});\n"
    );
    assert_entry_related("call-through-spy", COMMAND, &through)?;
    // Deleting from an instance removes only an own property, so the
    // prototype method still runs.
    let instance_deleted = format!(
        "{HELPER_SPY_TEST_HEAD}  delete program._collect;\n  program.add('a');\n  expect(program.items).toEqual(['a']);\n}});\n"
    );
    assert_entry_related("instance-deleted", COMMAND, &instance_deleted)?;
    // A comparison is not an assignment.
    let compared = format!(
        "{HELPER_SPY_TEST_HEAD}  const same = program._collect === undefined;\n  program.add('a');\n  expect(program.items).toEqual(['a']);\n}});\n"
    );
    assert_entry_related("comparison", COMMAND, &compared)
}

#[test]
fn a_second_class_with_the_same_name_does_not_lend_its_methods() {
    // Two classes called `Command` in one file share the `(file, name)` key
    // (#9942 item 6): the second one's `run` must not read as an entry of the
    // first one's `_collect`.
    let source = format!(
        "{COMMAND}\nfunction other() {{\n  class Command {{\n    run(value: string) {{\n      return this._collect(value, []);\n    }}\n  }}\n  return Command;\n}}\n"
    );
    assert!(entry_names(&source, "_collect").is_empty());
    let nested_first = format!(
        "function other() {{\n  class Command {{\n    run(value: string) {{\n      return this._collect(value, []);\n    }}\n  }}\n  return Command;\n}}\n\n{COMMAND}"
    );
    assert!(entry_names(&nested_first, "_collect").is_empty());
}

#[test]
fn a_default_parameter_with_an_arrow_type_does_not_make_a_method_a_generator() {
    // `=>` inside the parameter list must not close the parenthesis early and
    // expose a later `*` (a multiplication default) to the generator check.
    let source = COMMAND.replace(
        "  add(value: string): this {",
        "  add(value: string, done: () => void = () => {}, scale = 2 * 3): this {",
    );
    assert_eq!(entry_names(&source, "_collect"), ["add"]);
}

#[test]
fn only_the_first_argument_of_a_synchronous_callback_method_runs() {
    // `map`'s second argument is `thisArg`, a stored value, and a later
    // argument of the other methods is not the callback either.
    for (label, line) in [
        (
            "this-arg",
            "    [value].map((item) => item, () => this._collect(value, this.items));",
        ),
        (
            "second-argument-of-reduce",
            "    [value].reduce(() => 0, () => this._collect(value, this.items));",
        ),
    ] {
        assert!(
            entry_names(&with_call(line), "_collect").is_empty(),
            "{label}"
        );
    }
}

#[test]
fn a_parameter_default_of_an_unused_function_does_not_run() {
    // The default runs only when the function is called.
    let source = with_call(
        "    const later = function (seed = this._collect(value, this.items)) {\n      return seed;\n    };",
    );
    assert!(entry_names(&source, "_collect").is_empty());
    // A real call beside it still counts.
    let beside = with_call(
        "    const later = function (seed = this._collect(value, this.items)) {\n      return seed;\n    };\n    this.items = this._collect(value, this.items);",
    );
    assert_eq!(entry_names(&beside, "_collect"), ["add"]);
}
