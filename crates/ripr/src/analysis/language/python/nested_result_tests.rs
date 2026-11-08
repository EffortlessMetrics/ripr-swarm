//! Pins for the nested-result gate's effect-free `return` requirement (#12182).

use super::nested_result::is_effect_free_return;

#[test]
fn plain_returns_without_a_call_are_effect_free() {
    for line in [
        "        return m + 1",
        "        return f\"@{m}\"",
        "        return m * 2 if flag else 0",
        "return 3",
    ] {
        assert!(is_effect_free_return(line), "{line} reads values only");
    }
}

#[test]
fn returns_that_may_run_code_are_not_effect_free() {
    for (line, why) in [
        ("        return values.append(2)", "mutates through a call"),
        ("        return helper(m)", "calls a function"),
        ("        return (yield m)", "yields"),
        ("        return await fetch", "awaits"),
        ("        return lambda: 1", "builds a lambda"),
        ("        return (n := m)", "binds with a walrus"),
        ("        total = m + 1", "is not a return"),
        ("        returned = m", "only starts like a return"),
    ] {
        assert!(!is_effect_free_return(line), "{line} {why}");
    }
}
