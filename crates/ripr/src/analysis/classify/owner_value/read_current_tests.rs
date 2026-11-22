//! #11738: the current owner is read as the closed subset once per run
//! scope, and every answer is the direct reading's.

use super::{CLOSED_OWNER_READS, ClosedChange, ClosedOwner};
use crate::analysis::syntax::run_memo;

const OWNERS: &[(&str, &str)] = &[
    (
        "pub fn checked_div(a: u32, b: u32) -> Result<u32, ()> {\n    if b == 0 {\n        return Err(());\n    }\n    Ok(a / b)\n}\n",
        "checked_div",
    ),
    ("fn inc(x: u32) -> u32 {\n    x + 1\n}\n", "inc"),
    ("fn inc(x: u32) -> u32 {\n    x + 1\n}\n", "dec"),
    (
        "fn pick(x: u64) -> u64 {\n    if x > 3 {\n        return x + 2;\n    }\n    x * 2\n}\n",
        "pick",
    ),
    ("fn f(a: u32, b: u64) -> u64 { a }", "f"),
    ("fn g(x: u32) -> u32 { let y = x; y }", "g"),
    ("fn broken(x: u32) -> u32 }", "broken"),
    ("", "empty"),
];

/// The direct readings `ClosedChange` made before #11738.
fn reference_current(definition: &str, name: &str) -> Option<ClosedChange> {
    let owner = ClosedOwner::read(definition, name)?;
    Some(ClosedChange {
        before: owner.clone(),
        after: owner,
    })
}

fn reference_change(
    definition: &str,
    name: &str,
    changed_line: usize,
    before: &str,
) -> Option<ClosedChange> {
    if before.trim().is_empty() || before.contains('\n') {
        return None;
    }
    let mut lines: Vec<&str> = definition.lines().collect();
    *lines.get_mut(changed_line)? = before.trim_end();
    Some(ClosedChange {
        after: ClosedOwner::read(definition, name)?,
        before: ClosedOwner::read(&lines.join("\n"), name)?,
    })
}

fn same<T: std::fmt::Debug>(left: &T, right: &T) -> bool {
    format!("{left:?}") == format!("{right:?}")
}

fn check_every_owner() -> usize {
    let mut read = 0;
    for (definition, name) in OWNERS {
        let direct = ClosedOwner::read(definition, name);
        read += usize::from(direct.is_some());
        assert!(
            same(&ClosedOwner::read_current(definition, name), &direct),
            "{name}"
        );
        assert!(
            same(
                &ClosedChange::current(definition, name),
                &reference_current(definition, name)
            ),
            "{name}"
        );
        for changed_line in 0..definition.lines().count() + 1 {
            for before in [
                "    x + 2",
                "    Ok(a / (b + 1))",
                "        return Err(());",
                "",
                "a\nb",
            ] {
                assert!(
                    same(
                        &ClosedChange::read(definition, name, changed_line, before),
                        &reference_change(definition, name, changed_line, before)
                    ),
                    "{name} line {changed_line} before {before:?}"
                );
            }
        }
    }
    read
}

#[test]
fn the_current_reading_equals_the_direct_reading_inside_and_outside_a_scope() {
    // Outside a scope every call reads directly.
    let readable = check_every_owner();
    // Both answers must occur, or the table proves nothing.
    assert!(readable >= 3, "only {readable} owners read");
    assert!(readable < OWNERS.len(), "every owner read");
    let _scope = run_memo::enter();
    assert_eq!(check_every_owner(), readable);
    // Warm entries answer the same.
    assert_eq!(check_every_owner(), readable);
}

#[test]
fn an_owner_text_is_read_once_per_scope_however_many_lines_ask() {
    let reads = || CLOSED_OWNER_READS.with(std::cell::Cell::get);
    let (definition, name) = OWNERS[3];
    let _scope = run_memo::enter();
    let start = reads();
    for _ in 0..40 {
        assert!(ClosedChange::current(definition, name).is_some());
    }
    assert_eq!(reads() - start, 1, "forty asks, one read");
    // Each changed line still reads its own pre-change text once; the current
    // text is not read again.
    let start = reads();
    for changed_line in 1..4 {
        let _ = ClosedChange::read(definition, name, changed_line, "        return x + 3;");
    }
    assert_eq!(
        reads() - start,
        3,
        "one pre-change read per line, no current read"
    );
    // Another name over the same text is its own entry.
    let start = reads();
    assert!(ClosedChange::current(definition, "other").is_none());
    assert_eq!(reads() - start, 1);
}
