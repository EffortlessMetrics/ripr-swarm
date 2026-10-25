//! A literal zero-argument `patch` keeps its credit only when the edit cannot
//! hide behind it (RIPR-SPEC-0238 rule 8, `literal_patch.rs`).

use super::assertion_selection::PythonAssertionFocus;
use super::owners_tests::{extract_owners, extract_tests};
use super::related_tests::related_test_candidates;
use crate::domain::StaticLimitKind;
use std::path::Path;

const IMPORTS: &str = "from unittest.mock import patch\n\nfrom idioms.clock import stamp\n\n\n";

fn clock_test(patch_line: &str) -> String {
    format!(
        "{IMPORTS}def test_stamp_uses_the_patched_clock():\n    {patch_line}\n        assert stamp(\"a\") == \"a-1700000000\"\n"
    )
}

const CLOCK_PATCH: &str = "with patch(\"idioms.clock.time.time\", return_value=1700000000):";

/// The limit kind the change to line 5 of `idioms/clock.py` earns under the
/// test, with the owner body `body` (its last line is the changed line).
fn limit(body: &str, old: &str, test_source: &str) -> Option<StaticLimitKind> {
    let new = body.lines().last().unwrap_or_default();
    let owner_source = format!("import time\n\n\ndef stamp(prefix):\n{body}\n");
    let owners = extract_owners(Path::new("idioms/clock.py"), &owner_source);
    let owner = owners.iter().find(|owner| owner.name == "stamp");
    assert!(owner.is_some(), "owner stamp in {owner_source}");
    let owner = owner?;
    let tests = extract_tests(Path::new("tests/test_clock.py"), test_source);
    assert_eq!(tests.len(), 1, "{test_source}");
    let candidates = related_test_candidates(owner, &tests);
    assert!(!candidates.is_empty(), "test must relate: {test_source}");
    let focus = PythonAssertionFocus::for_change(
        super::probe_shape::classify_probe_shape(new).0,
        new,
        Some(old),
    );
    super::static_limits::static_limit_for_focused_change(
        new,
        Some(old),
        None,
        &focus,
        owner,
        &owner_source,
        &candidates,
    )
    .map(|limit| limit.kind)
}

const OLD: &str = "    return f\"{prefix}-{int(time.time())}\"";

#[test]
fn an_edit_around_a_fixed_clock_keeps_its_credit() {
    let test = clock_test(CLOCK_PATCH);
    for new in [
        "    return f\"{prefix}_{int(time.time())}\"",
        "    return f\"{prefix}-{int(time.time()) + 1}\"",
    ] {
        assert_eq!(limit(new, OLD, &test), None, "{new}");
    }
}

#[test]
fn an_edit_to_the_patched_calls_arguments_keeps_the_limit() {
    let test = clock_test(CLOCK_PATCH);
    // The mock ignores whatever the call is given, so the edit is invisible.
    assert_eq!(
        limit(
            "    return f\"{prefix}-{int(time.time(prefix))}\"",
            OLD,
            &test
        ),
        Some(StaticLimitKind::MockedModule)
    );
    assert_eq!(
        limit(
            OLD,
            "    return f\"{prefix}-{int(time.time(prefix))}\"",
            &test
        ),
        Some(StaticLimitKind::MockedModule)
    );
    // The patched call itself is removed or the name is used as a value.
    for new in [
        "    return f\"{prefix}-{int(0)}\"",
        "    return f\"{prefix}-{int(time.time)}\"",
    ] {
        assert_eq!(
            limit(new, OLD, &test),
            Some(StaticLimitKind::MockedModule),
            "{new}"
        );
    }
}

/// A same-named attribute of another object is not the substituted call; the
/// import-graph limit still applies to the line, so it is not credited.
#[test]
fn a_same_named_attribute_of_another_object_is_not_credited() {
    let test = clock_test(CLOCK_PATCH);
    assert!(
        limit(
            "    return f\"{prefix}-{int(self.time.time())}\"",
            OLD,
            &test
        )
        .is_some()
    );
}

#[test]
fn a_substitution_that_is_not_a_literal_zero_argument_stand_in_keeps_the_limit() {
    for patch_line in [
        "with patch(\"idioms.clock.time.time\", side_effect=[1]):",
        "with patch(\"idioms.clock.time.time\", return_value=NOW):",
        "with patch(\"idioms.clock.time.time\", return_value=1) as clock:",
        "with patch(\"idioms.clock.time.time\", 1700000000):",
        "with patch.object(time, \"time\", return_value=1):",
        "with patch(\"idioms.clock.time.time\"):",
        "with patch(\"idioms.clock.time.time\", return_value=1), patch(\"idioms.clock.os.getpid\", return_value=1):",
        "with patch(\"idioms.clock.stamp\", return_value=\"a-1\"):",
        "with patch(\"idioms.clock.time.time\", return_value=1, autospec=True):",
    ] {
        assert_eq!(
            limit(
                "    return f\"{prefix}_{int(time.time())}\"",
                OLD,
                &clock_test(patch_line)
            ),
            Some(StaticLimitKind::MockedModule),
            "{patch_line}"
        );
    }
}

#[test]
fn a_patch_that_is_not_unittest_mock_or_is_decorated_keeps_the_limit() {
    let new = "    return f\"{prefix}_{int(time.time())}\"";
    let other_patch = format!(
        "from other import patch\n\nfrom idioms.clock import stamp\n\n\ndef test_x():\n    {CLOCK_PATCH}\n        assert stamp(\"a\") == \"a-1700000000\"\n"
    );
    assert_eq!(
        limit(new, OLD, &other_patch),
        Some(StaticLimitKind::MockedModule)
    );
    let decorated = format!(
        "{IMPORTS}@patch(\"idioms.clock.time.time\", return_value=1700000000)\ndef test_x(mock_time):\n    assert stamp(\"a\") == \"a-1700000000\"\n"
    );
    assert_eq!(
        limit(new, OLD, &decorated),
        Some(StaticLimitKind::MockedModule)
    );
    let extra_mock = clock_test(CLOCK_PATCH)
        .replace("assert stamp", "helper = MagicMock()\n        assert stamp");
    assert_eq!(
        limit(new, OLD, &extra_mock),
        Some(StaticLimitKind::MockedModule)
    );
}

#[test]
fn a_changed_line_with_control_flow_or_other_uses_of_the_name_keeps_the_limit() {
    let test = clock_test(CLOCK_PATCH);
    // Control flow on the changed line: a fixed value could steer around the edit.
    for (old, new) in [
        (
            "    return prefix if time.time() > 5 else 'x'",
            "    return prefix + 'y' if time.time() > 5 else 'x'",
        ),
        (
            "    return prefix and time.time()",
            "    return prefix or time.time()",
        ),
    ] {
        assert_eq!(
            limit(new, old, &test),
            Some(StaticLimitKind::MockedModule),
            "{new}"
        );
    }
    // The name is also read on an unchanged line of the owner.
    let body = "    started = time.time()\n    return f\"{prefix}_{int(time.time())}\"";
    assert_eq!(limit(body, OLD, &test), Some(StaticLimitKind::MockedModule));
}

/// An operator that can absorb the edit under a fixed literal keeps the limit.
#[test]
fn an_edit_a_fixed_literal_can_absorb_keeps_the_limit() {
    let test = clock_test(CLOCK_PATCH);
    for (old, new) in [
        (
            "    return max(time.time(), 100)",
            "    return max(time.time(), 200)",
        ),
        (
            "    return int(time.time()) // 3600",
            "    return int(time.time()) // 60000",
        ),
        ("    return time.time() * 3", "    return time.time() * 4"),
        ("    return time.time() % 7", "    return time.time() % 9"),
        (
            "    return [1, 2, 3][int(time.time()) % 3]",
            "    return [1, 9, 3][int(time.time()) % 3]",
        ),
        // Changing the wrapper around the call (int to float) is invisible.
        (
            "    return int(time.time()) + 1",
            "    return float(time.time()) + 1",
        ),
        (
            "    return int(time.time()) + 1",
            "    return time.time() + 1",
        ),
    ] {
        assert_eq!(
            limit(new, old, &test),
            Some(StaticLimitKind::MockedModule),
            "{old} -> {new}"
        );
    }
    // An assignment whose result a later line branches on is not a return.
    assert_eq!(
        limit(
            "    ts = int(time.time()) // 60\n    if ts > 100:\n        return 'late'\n    return 'early'",
            "    ts = int(time.time()) // 3600",
            &test
        ),
        Some(StaticLimitKind::MockedModule)
    );
}

/// A float or boolean stand-in can absorb a small term (`1e16 + 1 == 1e16`).
#[test]
fn a_non_integer_stand_in_keeps_the_limit() {
    for patch_line in [
        "with patch(\"idioms.clock.time.time\", return_value=1e16):",
        "with patch(\"idioms.clock.time.time\", return_value=True):",
        "with patch(\"idioms.clock.time.time\", return_value=None):",
    ] {
        assert_eq!(
            limit(
                "    return f\"{prefix}_{int(time.time())}\"",
                OLD,
                &clock_test(patch_line)
            ),
            Some(StaticLimitKind::MockedModule),
            "{patch_line}"
        );
    }
}

/// An empty string stand-in contributes nothing to the value, so moving the
/// call among the string parts is invisible to the test.
#[test]
fn an_empty_string_stand_in_keeps_the_limit() {
    let empty = clock_test("with patch(\"idioms.clock.time.time\", return_value=\"\"):");
    for (new, old) in [
        (
            "    return f\"{prefix}{time.time()}\"",
            "    return f\"{time.time()}{prefix}\"",
        ),
        (
            "    return prefix + \"x\" + time.time()",
            "    return prefix + time.time() + \"x\"",
        ),
        (
            "    return prefix + time.time()",
            "    return time.time() + prefix",
        ),
    ] {
        assert_eq!(
            limit(new, old, &empty),
            Some(StaticLimitKind::MockedModule),
            "{new}"
        );
    }
}

/// With a zero stand-in `x + v` and `x - v` agree, so an operator flip is
/// invisible; a nonzero one keeps it visible.
#[test]
fn a_zero_stand_in_keeps_the_limit() {
    let zero = clock_test("with patch(\"idioms.clock.time.time\", return_value=0):");
    assert_eq!(
        limit(
            "    return prefix - int(time.time())",
            "    return prefix + int(time.time())",
            &zero
        ),
        Some(StaticLimitKind::MockedModule)
    );
    // A sign flip is not an edit the exemption looks through at all.
    assert_eq!(
        limit(
            "    return prefix - int(time.time())",
            "    return prefix + int(time.time())",
            &clock_test(CLOCK_PATCH)
        ),
        Some(StaticLimitKind::MockedModule)
    );
}

/// One call, same sign: `v + v` and `v - v + 2` agree at `v = 1` (the review's
/// collision), and a sign flip can be offset by a constant.
#[test]
fn an_arithmetic_collision_at_the_fixed_value_keeps_the_limit() {
    let one = clock_test("with patch(\"idioms.clock.time.time\", return_value=1):");
    assert_eq!(
        limit(
            "    return time.time() - time.time() + 2",
            "    return time.time() + time.time()",
            &one
        ),
        Some(StaticLimitKind::MockedModule)
    );
    assert_eq!(
        limit(
            "    return 12 - time.time()",
            "    return 10 + time.time()",
            &one
        ),
        Some(StaticLimitKind::MockedModule)
    );
}

/// A test line that can rebind names (`stamp.__globals__[...] = lambda: 3`)
/// is not a plain test body.
#[test]
fn a_test_that_rebinds_names_keeps_the_limit() {
    let new = "    return f\"{prefix}_{int(time.time())}\"";
    for extra in [
        "stamp.__globals__['stamp'] = lambda: 3",
        "stamp.__code__ = other.__code__",
        "globals()['stamp'] = print",
        "x = globals()",
    ] {
        let test = clock_test(CLOCK_PATCH)
            .replace("assert stamp", &format!("{extra}\n        assert stamp"));
        assert_eq!(
            limit(new, OLD, &test),
            Some(StaticLimitKind::MockedModule),
            "{extra}"
        );
    }
}

/// Statements that could rebind the owner behind the patch are not a plain
/// test body: a second statement on a line, a chained or attribute
/// assignment, a helper call, a default argument, a nested def.
#[test]
fn a_test_body_that_can_rebind_the_owner_keeps_the_limit() {
    let new = "    return f\"{prefix}_{int(time.time())}\"";
    let with_line = |extra: &str| {
        clock_test(CLOCK_PATCH).replace("assert stamp", &format!("{extra}\n        assert stamp"))
    };
    for extra in [
        "x = 1; idioms.clock.stamp = ref",
        "x = idioms.clock.stamp = ref",
        "idioms.clock.stamp = ref",
        "x = rig()",
    ] {
        assert_eq!(
            limit(new, OLD, &with_line(extra)),
            Some(StaticLimitKind::MockedModule),
            "{extra}"
        );
    }
    // A pure attribute read cannot rebind anything.
    assert_eq!(limit(new, OLD, &with_line("x = stamp.attr")), None);
    let semicolon = clock_test(CLOCK_PATCH).replace(
        "assert stamp(\"a\") == \"a-1700000000\"",
        "assert stamp(\"a\") == \"a-1700000000\"; idioms.clock.stamp = ref",
    );
    assert_eq!(
        limit(new, OLD, &semicolon),
        Some(StaticLimitKind::MockedModule)
    );
    let default_argument = clock_test(CLOCK_PATCH).replace(
        "def test_stamp_uses_the_patched_clock():",
        "def test_stamp_uses_the_patched_clock(_=rig()):",
    );
    assert_eq!(
        limit(new, OLD, &default_argument),
        Some(StaticLimitKind::MockedModule)
    );
    // `patch` bound twice is not `unittest.mock.patch`.
    let shadowed = clock_test(CLOCK_PATCH).replace(
        "from unittest.mock import patch\n",
        "from unittest.mock import patch\nfrom mylib import patch\n",
    );
    assert_eq!(
        limit(new, OLD, &shadowed),
        Some(StaticLimitKind::MockedModule)
    );
}
