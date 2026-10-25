use super::*;

const AUDITED: &str = r#"
export function audited(_target: object, key: string, descriptor: PropertyDescriptor): void {
  const original = descriptor.value as (...args: number[]) => number;
  descriptor.value = function (this: unknown, ...args: number[]): number {
    calls.push(key);
    return original.apply(this, args) + 1;
  };
}

export class Ledger {
  @audited
  add(a: number, b: number): number {
    return a + b;
  }
}
"#;

fn decorator_entries(source: &str) -> Vec<TypeScriptModuleEntry> {
    extract_owners(Path::new("src/audited.ts"), source)
        .into_iter()
        .find(|owner| owner.name == "audited" && owner.owner_kind == OwnerKind::Function)
        .map(|owner| owner.module_entries)
        .unwrap_or_default()
}

fn assert_no_decorated_entry(source: &str) {
    assert!(
        decorator_entries(source)
            .iter()
            .all(|entry| entry.decorated.is_none()),
        "expected no decorated entry for:\n{source}"
    );
}

#[test]
fn pass_through_decorator_records_its_decorated_method_and_return_lines() {
    let entries = decorator_entries(AUDITED);
    assert_eq!(entries.len(), 1, "expected one entry, got {entries:?}");
    assert_eq!(entries[0].name, "add");
    assert_eq!(
        entries[0].decorated,
        Some(TypeScriptDecoratedMethod {
            class_name: "Ledger".to_string(),
            return_start_line: 6,
            return_end_line: 6,
        })
    );
}

#[test]
fn result_replacing_wrapper_records_no_entry() {
    assert_no_decorated_entry(&AUDITED.replace(
        "return original.apply(this, args) + 1;",
        "return cache.get(args[0]) as number;",
    ));
}

#[test]
fn conditional_or_logical_return_records_no_entry() {
    for returned in [
        "return cache.has(1) ? cache.get(1) : original.apply(this, args);",
        "return original.apply(this, args) || 0;",
        "return stored && original.apply(this, args);",
    ] {
        assert_no_decorated_entry(
            &AUDITED.replace("return original.apply(this, args) + 1;", returned),
        );
    }
}

#[test]
fn wrapper_reading_outside_state_records_no_entry() {
    assert_no_decorated_entry(&AUDITED.replace(
        "return original.apply(this, args) + 1;",
        "return original.apply(this, args) + offset;",
    ));
}

#[test]
fn wrapper_with_earlier_control_flow_or_async_records_no_entry() {
    assert_no_decorated_entry(
        &AUDITED.replace("calls.push(key);", "if (args.length === 0) { return 0; }"),
    );
    assert_no_decorated_entry(&AUDITED.replace("function (this", "async function (this"));
    assert_no_decorated_entry(&AUDITED.replace(
        "return original.apply(this, args) + 1;",
        "try { return original.apply(this, args); } finally { calls.push(key); }",
    ));
}

#[test]
fn descriptor_members_beyond_value_record_no_entry() {
    assert_no_decorated_entry(&AUDITED.replace(
        "calls.push(key);",
        "descriptor.writable = false; calls.push(key);",
    ));
    assert_no_decorated_entry(&AUDITED.replace(
        "const original =",
        "descriptor.enumerable = false; const original =",
    ));
}

#[test]
fn only_a_lone_bare_method_decorator_is_an_application() {
    for class in [
        "@audited(3)\n  add(a: number, b: number) { return a + b; }",
        "@other\n  @audited\n  add(a: number, b: number) { return a + b; }",
        "@audited\n  static add(a: number, b: number) { return a + b; }",
        "@audited\n  get total() { return 1; }",
        "@audited\n  amount = 1;",
    ] {
        let source = AUDITED.replace(
            "@audited\n  add(a: number, b: number): number {\n    return a + b;\n  }",
            class,
        );
        assert_no_decorated_entry(&source);
    }
    assert_no_decorated_entry(
        &AUDITED.replace("export class Ledger", "@sealed\nexport class Ledger"),
    );
}

#[test]
fn a_decorator_with_two_declarations_or_no_application_records_no_entry() {
    let duplicated =
        format!("{AUDITED}\nfunction audited(a: object, b: string, c: PropertyDescriptor) {{}}\n");
    assert_no_decorated_entry(&duplicated);
    assert_no_decorated_entry(&AUDITED.replace("@audited\n  add", "add"));
}

#[test]
fn decorated_call_must_be_on_a_receiver_of_the_decorated_class() {
    use super::super::classifier::ts_observes_decorated_method_call as observes;
    let body = "const l = new Ledger();\n  l.add(1, 1);";
    assert!(observes("new Ledger().add(2, 3)", "", "Ledger", "add"));
    assert!(observes("new Ledger(5).add(2, 3)", "", "Ledger", "add"));
    assert!(observes("l.add(2, 3)", body, "Ledger", "add"));
    assert!(!observes("new Other().add(2, 3)", body, "Ledger", "add"));
    assert!(!observes("o.add(2, 3)", body, "Ledger", "add"));
    assert!(!observes("new Ledger().total(2, 3)", body, "Ledger", "add"));
    assert!(!observes("makeLedger().add(2, 3)", body, "Ledger", "add"));
}

#[test]
fn decorated_call_refuses_a_method_replaced_in_the_test() {
    use super::super::classifier::ts_observes_decorated_method_call as observes;
    for body in [
        "const l = new Ledger();\n  l.add = () => 99;",
        "const l = new Ledger();\n  Ledger.prototype.add = () => 99;",
        "const l = new Ledger();\n  jest.spyOn(l, \"add\").mockReturnValue(99);",
        "const l = new Ledger();\n  l. add = () => 99;",
        "const l = new Ledger();\n  l\n    .add = () => 99;",
        "const l = new Ledger();\n  l./* swap */add = () => 99;",
        "const l = new Ledger();\n  l['add'] = () => 99;",
        "const l = new Ledger();\n  l[name] = () => 99;",
        "const l = new Ledger();\n  [l.add] = [() => 99];",
        "const l = new Ledger();\n  ({ add: l.add } = { add: () => 99 });",
        "const l = new Ledger();\n  run(l.add);",
        "const l = new Ledger();\n  Object.assign(l, { add: () => 99 });",
        "const l = new Ledger();\n  Object.defineProperty(l, 'add', { value: () => 99 });",
    ] {
        assert!(!observes("l.add(2, 3)", body, "Ledger", "add"), "{body}");
    }
    assert!(observes(
        "l.add(2, 3)",
        "const l = new Ledger();\n  const added = l.add(2, 3);",
        "Ledger",
        "add"
    ));
}

#[test]
fn replacement_descriptor_or_outer_control_flow_records_no_entry() {
    assert_no_decorated_entry(&AUDITED.replace(
        "  };\n}\n",
        "  };\n  return { value: function () { return 99; } };\n}\n",
    ));
    assert_no_decorated_entry(&AUDITED.replace(
        "  descriptor.value = function",
        "  if (key === \"x\") { return; }\n  descriptor.value = function",
    ));
    assert_no_decorated_entry(&AUDITED.replace(
        "  descriptor.value = function",
        "  Object.defineProperty(target, key, { value: 1 });\n  descriptor.value = function",
    ));
}

#[test]
fn returned_value_must_be_the_originals_result_combined_only_by_add_or_subtract() {
    for returned in [
        "return (original.apply(this, args) + 1, 99);",
        "return [original.apply(this, args)][0];",
        "return original.apply(this, args) * 0 + 99;",
        "return original.apply(this, args) + original.apply(this, args);",
        "return 99 + 1;",
        "return original.apply(null, args) + 1;",
        "return original?.apply(this, args) + 1;",
        "return String(original.apply(this, args));",
    ] {
        assert_no_decorated_entry(
            &AUDITED.replace("return original.apply(this, args) + 1;", returned),
        );
    }
    for returned in [
        "return original.apply(this, args);",
        "return original.call(this, key) - 1;",
        "return (original.apply(this, args) as number) + 1;",
    ] {
        assert_eq!(
            decorator_entries(&AUDITED.replace("return original.apply(this, args) + 1;", returned))
                .len(),
            1,
            "{returned}"
        );
    }
}

#[test]
fn decorated_call_must_be_the_whole_asserted_value_on_a_single_bound_receiver() {
    use super::super::classifier::ts_observes_decorated_method_call as observes;
    let body = "const l = new Ledger();";
    assert!(observes("l.add(2, 3)", body, "Ledger", "add"));
    assert!(observes("(l.add(2, 3))", body, "Ledger", "add"));
    assert!(observes("await l.add(2, 3)", body, "Ledger", "add"));
    for observed in [
        "typeof new Ledger().add(2, 3)",
        "new Ledger().add(2, 3) > 0",
        "true ? 5 : l.add(2, 3)",
        "5 || l.add(2, 3)",
        "l.add(2, 3) + l.add(1, 1)",
    ] {
        assert!(!observes(observed, body, "Ledger", "add"), "{observed}");
    }
    // A receiver that is reassigned, redeclared in a nested scope, or bound
    // as a parameter is not known to be the decorated class.
    for body in [
        "let l = new Ledger();\n  l = { add: (x, y) => x + y };",
        "const l = new Ledger();\n  { const l = { add: (x, y) => x + y }; }",
        "const l = new Ledger();\n  run((l) => 1);",
        "l = new Ledger();",
    ] {
        assert!(!observes("l.add(2, 3)", body, "Ledger", "add"), "{body}");
    }
}
