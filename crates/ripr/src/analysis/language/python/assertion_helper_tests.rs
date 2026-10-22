//! Same-file assertion-helper credit must retain the changed receiver and field.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER: &str = "class Stats:\n    def __init__(self):\n        self.writes = 0\n        self.hits = 0\n\n    def record_write(self):\n        self.writes += 1\n";
const HELPER: &str = "def check_writes(obj, want):\n    assert obj.writes == want\n";

fn test_source(helper: &str, call: &str) -> String {
    format!(
        "from src.stats import Stats\n\n{helper}\ndef test_writes():\n    stats = Stats()\n    stats.record_write()\n    stats.record_write()\n    {call}\n"
    )
}

fn finding(source: &str) -> Result<Finding, String> {
    finding_with_owner(source, OWNER)
}

fn finding_with_owner(source: &str, owner_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/stats.py"), owner_source);
    let tests = extract_tests(Path::new("tests/test_stats.py"), source);
    assert!(owners.iter().any(|owner| owner.name == "record_write"));
    assert_eq!(tests.len(), 1, "{source}");
    classify_change_with_old(
        Path::new("src/stats.py"),
        7,
        "        self.writes += 1",
        Some("        self.writes = self.writes + 1"),
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed field write must produce a finding".to_string())
}

#[test]
fn a_same_file_assertion_helper_refuses_deferred_owner_bodies() -> Result<(), String> {
    let source = test_source(
        &format!("{HELPER}\ndef drop(coro):\n    coro.close()\n"),
        "check_writes(stats, 0)",
    )
    .replace(
        "    stats.record_write()\n",
        "    drop(stats.record_write())\n",
    );
    for owner in [
        OWNER.replace("def record_write(", "async def record_write("),
        format!("{OWNER}        yield None\n"),
        format!("{OWNER}        f\"{{(yield None)}}\"\n"),
    ] {
        assert_eq!(
            finding_with_owner(&source, &owner)?.class,
            ExposureClass::WeaklyExposed,
            "{owner}"
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_observes_the_written_field() -> Result<(), String> {
    let source = test_source(HELPER, "check_writes(stats, 2)");
    assert_eq!(finding(&source)?.class, ExposureClass::Exposed);
    let tests = extract_tests(Path::new("tests/test_stats.py"), &source);
    let assertion = tests[0]
        .assertions
        .iter()
        .find(|assertion| assertion.oracle_strength == OracleStrength::Strong)
        .ok_or("the helper must supply a strong instantiated assertion")?;
    assert_eq!(assertion.gate_text, "assert stats.writes == 2");
    assert_eq!(
        assertion.line,
        source
            .lines()
            .position(|line| line.trim() == "check_writes(stats, 2)")
            .ok_or("the test must contain the helper call")?
            + 1
    );
    for (helper, call) in [
        (
            "def check_writes(obj):\n    assert obj.writes == 2\n",
            "check_writes(stats)",
        ),
        (
            "def check_writes(want, obj):\n    assert want == obj.writes\n",
            "check_writes(2, stats)",
        ),
    ] {
        assert_eq!(
            finding(&test_source(helper, call))?.class,
            ExposureClass::Exposed
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_cannot_borrow_a_receiver_or_field() -> Result<(), String> {
    for source in [
        test_source(HELPER, "other = Stats()\n    check_writes(other, 0)"),
        test_source(
            "def check_writes(obj, want):\n    assert obj.hits == want\n",
            "check_writes(stats, 0)",
        ),
    ] {
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_keeps_surviving_mutant_controls_weak() -> Result<(), String> {
    // Deleting record_write's update still satisfies either helper.
    for helper in [
        "def check_writes(obj, want):\n    assert obj.writes >= want\n",
        "def check_writes(obj, want):\n    assert obj.writes == obj.writes\n",
    ] {
        let source = test_source(helper, "check_writes(stats, 0)");
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_must_run_after_the_owner() -> Result<(), String> {
    let source = format!(
        "from src.stats import Stats\n\n{HELPER}\ndef test_writes():\n    stats = Stats()\n    check_writes(stats, 0)\n    stats.record_write()\n"
    );
    assert_eq!(finding(&source)?.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_opaque_or_rebound_definitions() -> Result<(), String> {
    for helper in [
        "def check_writes(obj, want):\n    if want:\n        assert obj.writes == want\n",
        "@wrap\ndef check_writes(obj, want):\n    assert obj.writes == want\n",
        "def check_writes(obj, want):\n    assert obj.writes == want\n    return True\n",
        "def check_writes(obj, want):\n    assert obj.writes + 1 == want\n",
        "def check_writes(obj, want):\n    assert obj.writes == want\n\ndef check_writes(obj, want):\n    pass\n",
        "def check_writes(obj, want):\n    assert obj.writes == want\n\ncheck_writes = lambda *args: None\n",
        "def check_writes(obj, want=2):\n    assert obj.writes == want\n",
        "from tests.helpers import check_writes\n",
    ] {
        let source = test_source(helper, "check_writes(stats, 2)");
        assert_ne!(finding(&source)?.class, ExposureClass::Exposed, "{source}");
    }
    for call in [
        "check_writes = lambda *args: None\n    check_writes(stats, 2)",
        "if False:\n        check_writes(stats, 2)",
        "return\n    check_writes(stats, 2)",
    ] {
        let source = test_source(HELPER, call);
        assert_ne!(finding(&source)?.class, ExposureClass::Exposed, "{source}");
    }
    for decorator in ["@wrap", "@pytest.mark.skip"] {
        let source = test_source(HELPER, "check_writes(stats, 2)").replace(
            "def test_writes():",
            &format!("{decorator}\ndef test_writes():"),
        );
        assert_ne!(finding(&source)?.class, ExposureClass::Exposed, "{source}");
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_unmapped_arguments() -> Result<(), String> {
    for call in [
        "check_writes(stats)",
        "check_writes(stats, 2, 3)",
        "check_writes(*[stats, 2])",
        "check_writes(obj=stats, want=2)",
    ] {
        let source = test_source(HELPER, call);
        assert_ne!(finding(&source)?.class, ExposureClass::Exposed, "{source}");
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_ambiguous_same_line_order() -> Result<(), String> {
    let source = format!(
        "from src.stats import Stats\n\n{HELPER}\ndef test_writes():\n    stats = Stats()\n    check_writes(stats, 0); stats.record_write()\n"
    );
    assert_eq!(finding(&source)?.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_modified_executable_identity() -> Result<(), String> {
    for replacement in [
        "check_writes.__code__ = ignore.__code__",
        "alias = check_writes\nalias.__code__ = ignore.__code__",
        "check_writes.__setattr__('__code__', ignore.__code__)",
        "check_writes.__globals__['check_writes'] = ignore",
    ] {
        let helper = format!("{HELPER}\ndef ignore(obj, want):\n    pass\n\n{replacement}\n");
        let source = test_source(&helper, "check_writes(stats, 2)");
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_inactive_owner_calls() -> Result<(), String> {
    for inactive in [
        "False and stats.record_write()",
        "stats.record_write() if False else None",
        "unused = (stats.record_write() for _ in [0])",
        "unused = lambda: stats.record_write()",
        "unused: stats.record_write()",
    ] {
        let source = format!(
            "from src.stats import Stats\n\n{HELPER}\ndef test_writes():\n    stats = Stats()\n    {inactive}\n    check_writes(stats, 0)\n"
        );
        assert_ne!(finding(&source)?.class, ExposureClass::Exposed, "{source}");
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_definition_time_calls() -> Result<(), String> {
    for header in [
        "(unused=stats.record_write())",
        "(unused: stats.record_write()=None)",
        "() -> stats.record_write()",
    ] {
        let source = format!(
            "from src.stats import Stats\n\n{HELPER}\nstats = Stats()\n\ndef test_writes{header}:\n    stats = Stats()\n    check_writes(stats, 0)\n"
        );
        assert_ne!(finding(&source)?.class, ExposureClass::Exposed, "{source}");
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_receiver_or_field_modification() -> Result<(), String> {
    for body in [
        "stats.record_write = ignore\n    stats.record_write()",
        "stats.__setattr__('record_write', ignore)\n    stats.record_write()",
        "stats.record_write()\n    stats.writes = 0",
    ] {
        let source = format!(
            "from src.stats import Stats\n\n{HELPER}\ndef ignore():\n    pass\n\ndef test_writes():\n    stats = Stats()\n    {body}\n    check_writes(stats, 0)\n"
        );
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
    }
    for replacement in [
        "Stats.record_write = ignore",
        "alias = Stats\nalias.record_write = ignore",
        "type.__setattr__(Stats, 'record_write', ignore)",
    ] {
        let source = format!(
            "from src.stats import Stats\n\n{HELPER}\ndef ignore(self):\n    pass\n\n{replacement}\n\ndef test_writes():\n    stats = Stats()\n    stats.record_write()\n    check_writes(stats, 0)\n"
        );
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
        let tests = extract_tests(Path::new("tests/test_stats.py"), &source);
        assert!(
            tests[0]
                .assertions
                .iter()
                .all(|assertion| !assertion.effect_only)
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_derived_expected_values() -> Result<(), String> {
    for source in [
        test_source(HELPER, "want = stats.writes\n    check_writes(stats, want)"),
        test_source(
            "def check_writes(obj, other):\n    assert obj.writes == other.writes\n",
            "other = stats\n    check_writes(stats, other)",
        ),
    ] {
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_receiver_rebinding() -> Result<(), String> {
    for replacement in [
        "saved = stats; stats = Other()",
        "saved = stats = Other()",
        "(stats) = Other()",
        "import stats",
        "from other import stats",
        "import other as stats",
        "from other import replacement as stats",
    ] {
        let source = format!(
            "from src.stats import Stats\n\n{HELPER}\nclass Other:\n    writes = 0\n\ndef test_writes():\n    stats = Stats()\n    stats.record_write()\n    {replacement}\n    check_writes(stats, 0)\n"
        );
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_literal_owner_calls() -> Result<(), String> {
    for literal in [
        "\"\"\"documentation:\nstats.record_write()\n\"\"\"",
        "'documentation:\\\nstats.record_write()'",
    ] {
        let source = format!(
            "from src.stats import Stats\n\n{HELPER}\ndef test_writes():\n    stats = Stats()\n    text = {literal}\n    check_writes(stats, 0)\n"
        );
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_refuses_shadowed_class_bindings() -> Result<(), String> {
    let other = "class Other:\n    writes = 0\n    def record_write(self):\n        pass\n";
    for (prefix, header, setup, expected) in [
        ("Stats = Other\n", "()", "", ExposureClass::WeaklyExposed),
        (
            "",
            "()",
            "Stats = Other\n    ",
            ExposureClass::WeaklyExposed,
        ),
        ("", "(Stats)", "", ExposureClass::StaticUnknown),
    ] {
        let source = format!(
            "from src.stats import Stats\n\n{HELPER}\n{other}\n{prefix}\ndef test_writes{header}:\n    {setup}stats = Stats()\n    stats.record_write()\n    check_writes(stats, 0)\n"
        );
        assert_eq!(finding(&source)?.class, expected, "{source}");
    }
    Ok(())
}

#[test]
fn a_same_file_assertion_helper_cannot_borrow_an_owner_name_token() -> Result<(), String> {
    let wrong_field = "def check_writes(obj, want):\n    assert obj.hits == want\n";
    let source = test_source(wrong_field, "check_writes(stats, 0)")
        .replace("stats =", "record_write =")
        .replace("stats.", "record_write.")
        .replace("check_writes(stats,", "check_writes(record_write,");
    for source in [
        test_source(
            "def check_writes(obj, want):\n    assert obj.record_write == want\n",
            "method = stats.record_write\n    check_writes(stats, method)",
        ),
        test_source(
            wrong_field,
            "stats.hits = 'record_write'\n    check_writes(stats, 'record_write')",
        ),
        source,
    ] {
        assert_eq!(
            finding(&source)?.class,
            ExposureClass::WeaklyExposed,
            "{source}"
        );
    }
    Ok(())
}
