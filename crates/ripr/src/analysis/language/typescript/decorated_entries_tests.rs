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
