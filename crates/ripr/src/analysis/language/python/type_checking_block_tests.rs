//! Tests for type-checking-only blocks (#14156): a changed line in the body
//! of `if TYPE_CHECKING:` never runs, so it is not a behavior probe.

use super::type_checking_block::type_checking_only_line_ranges;
use super::*;
use std::path::PathBuf;

/// Lines 6 to 10 are the block; `total` on 13 and 14 is runtime code.
const PRICING_PY: &str = "from __future__ import annotations\n\nfrom collections.abc import Iterable, Mapping, Sequence\nfrom typing import TYPE_CHECKING\n\nif TYPE_CHECKING:\n    from typing import TypeAlias\n\n    Prices: TypeAlias = Mapping[str, int]\n    Basket: TypeAlias = Sequence[str]\n\n\ndef total(prices: Prices, basket: Iterable[str]) -> int:\n    return sum(prices[item] for item in basket)\n";

const PRICING_TEST_PY: &str = "from src.pricing import total\n\n\ndef test_total():\n    assert total({\"a\": 2, \"b\": 3}, [\"a\", \"b\"]) == 5\n";

fn line(number: usize, text: &str) -> crate::analysis::diff::ChangedLine {
    crate::analysis::diff::ChangedLine {
        line: number,
        new_side_line: number,
        text: text.to_string(),
    }
}

fn analyze(
    label: &str,
    source: &str,
    added: &[usize],
    removed: Vec<crate::analysis::diff::ChangedLine>,
) -> Result<Vec<Finding>, String> {
    analyze_with(label, &[], source, added, removed)
}

/// `analyze` with extra workspace files beside the fixture.
fn analyze_with(
    label: &str,
    extra_files: &[(&str, &str)],
    source: &str,
    added: &[usize],
    removed: Vec<crate::analysis::diff::ChangedLine>,
) -> Result<Vec<Finding>, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time: {err}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-type-checking-{label}-{}-{nanos}",
        std::process::id()
    ));
    for (path, contents) in [
        ("src/pricing.py", source),
        ("tests/test_pricing.py", PRICING_TEST_PY),
    ]
    .into_iter()
    .chain(extra_files.iter().copied())
    {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create_dir_all({}): {err}", parent.display()))?;
        }
        std::fs::write(&file, contents)
            .map_err(|err| format!("write({}): {err}", file.display()))?;
    }
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let mut added_lines = Vec::new();
    for number in added {
        let text = source
            .lines()
            .nth(number - 1)
            .ok_or_else(|| format!("fixture has no line {number}"))?;
        added_lines.push(line(*number, text));
    }
    let changed_files = vec![ChangedFile {
        path: PathBuf::from("src/pricing.py"),
        added_lines,
        removed_lines: removed,
    }];
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let _ = std::fs::remove_dir_all(&root);
    let result = result?;
    assert_eq!(result.changed_files, 1, "the changed file must be analyzed");
    Ok(result.findings)
}

fn probed_lines(findings: &[Finding]) -> Vec<usize> {
    let mut lines: Vec<usize> = findings
        .iter()
        .map(|finding| finding.probe.location.line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

fn ranges(source: &str) -> Vec<std::ops::RangeInclusive<usize>> {
    type_checking_only_line_ranges(Path::new("src/pricing.py"), source)
}

/// The requests `5691f59` shape: a type alias under `if TYPE_CHECKING:` is
/// edited in place. Nothing runs differently, so there is no finding. Without
/// the guard line 10 reads as a `predicate` probe on `<module>`.
#[test]
fn edited_alias_in_a_type_checking_block_is_not_probed() -> Result<(), String> {
    let findings = analyze(
        "alias-edit",
        PRICING_PY,
        &[10],
        vec![line(10, "    Basket: TypeAlias = list[str]")],
    )?;
    assert_eq!(probed_lines(&findings), Vec::<usize>::new(), "{findings:?}");
    Ok(())
}

/// A whole new block: neither the always-false `if` nor its body is probed.
#[test]
fn added_type_checking_block_is_not_probed() -> Result<(), String> {
    let findings = analyze("new-block", PRICING_PY, &[6, 7, 8, 9, 10], Vec::new())?;
    assert_eq!(probed_lines(&findings), Vec::<usize>::new(), "{findings:?}");
    Ok(())
}

/// The control on the same file: the runtime line keeps its probe, and a
/// type-checking line changed beside it adds none.
#[test]
fn runtime_line_beside_a_type_checking_edit_keeps_its_probe() -> Result<(), String> {
    let findings = analyze(
        "runtime-control",
        PRICING_PY,
        &[10, 14],
        vec![
            line(10, "    Basket: TypeAlias = list[str]"),
            line(14, "    return sum(prices[item] for item in basket) + 1"),
        ],
    )?;
    assert_eq!(probed_lines(&findings), vec![14], "{findings:?}");
    Ok(())
}

/// Runtime code replaced by a line inside the block: the old line ran, so
/// the change is behavior and keeps a probe.
#[test]
fn runtime_code_replaced_by_a_type_checking_line_keeps_probe() -> Result<(), String> {
    let findings = analyze(
        "runtime-into-block",
        PRICING_PY,
        &[10],
        vec![line(10, "DISCOUNT = compute_discount()")],
    )?;
    assert_eq!(probed_lines(&findings), vec![10], "{findings:?}");
    Ok(())
}

/// A line deleted from the old file's block deleted no behavior.
#[test]
fn line_deleted_from_a_type_checking_block_is_not_probed() -> Result<(), String> {
    let mut removed = line(11, "    Rows: TypeAlias = list[Basket]");
    removed.new_side_line = 11;
    let findings = analyze("deleted-alias", PRICING_PY, &[], vec![removed])?;
    assert_eq!(probed_lines(&findings), Vec::<usize>::new(), "{findings:?}");
    Ok(())
}

/// Devin on #14455: runtime code moved into the block in the same added
/// run as a changed runtime line. The moved line replaced a line that ran,
/// so it keeps its probe beside the runtime edit.
#[test]
fn runtime_code_moved_into_a_block_beside_a_runtime_edit_keeps_probe() -> Result<(), String> {
    let findings = analyze(
        "moved-beside-runtime",
        PRICING_PY,
        &[10, 11, 12, 13, 14],
        vec![
            line(10, "DISCOUNT = compute_discount()"),
            line(14, "    return sum(prices[item] for item in basket) + 1"),
        ],
    )?;
    assert!(probed_lines(&findings).contains(&10), "{findings:?}");
    Ok(())
}

/// Devin on #14455: a workspace `typing.py` can make `TYPE_CHECKING` true,
/// so no block is read and the edited alias keeps its probe.
#[test]
fn workspace_typing_module_keeps_block_probes() -> Result<(), String> {
    let findings = analyze_with(
        "shadowed-typing",
        &[("typing.py", "TYPE_CHECKING = True\n")],
        PRICING_PY,
        &[10],
        vec![line(10, "    Basket: TypeAlias = list[str]")],
    )?;
    assert_eq!(probed_lines(&findings), vec![10], "{findings:?}");
    Ok(())
}

#[test]
fn workspace_typing_shadow_paths() {
    use super::type_checking_block::workspace_may_shadow_typing;
    for (paths, expected) in [
        (vec!["typing.py"], true),
        (vec!["src/typing.py"], true),
        (vec!["typing_extensions/__init__.py"], true),
        (vec!["src/typing_extensions.pyi"], true),
        (vec!["src/pricing.py", "tests/test_pricing.py"], false),
        (vec!["src/shop/typing.py"], false),
        (vec!["shop/_typing.py"], false),
    ] {
        let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        assert_eq!(workspace_may_shadow_typing(&paths), expected, "{paths:?}");
    }
}

#[test]
fn type_checking_block_shapes() {
    for (why, source, expected) in [
        (
            "from-import, one-line guard",
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    import a\n    import b\nx = 1\n",
            vec![2..=4],
        ),
        (
            "module attribute",
            "import typing\nif typing.TYPE_CHECKING:\n    import a\n",
            vec![2..=3],
        ),
        (
            "module alias",
            "import typing as t\nif t.TYPE_CHECKING:\n    import a\n",
            vec![2..=3],
        ),
        (
            "typing_extensions",
            "from typing_extensions import TYPE_CHECKING\nif TYPE_CHECKING:\n    import a\n",
            vec![2..=3],
        ),
        (
            "the import-free spelling",
            "TYPE_CHECKING = False\nif TYPE_CHECKING:\n    import a\n",
            vec![2..=3],
        ),
        (
            "the else branch runs",
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    import a\nelse:\n    a = None\n",
            vec![2..=3],
        ),
        (
            "inside a class body",
            "from typing import TYPE_CHECKING\nclass Shop:\n    if TYPE_CHECKING:\n        x: int\n    y = 1\n",
            vec![3..=4],
        ),
        (
            "a wrapped test keeps its own lines",
            "import typing\nif (\n    typing.TYPE_CHECKING\n):\n    import a\n",
            vec![5..=5],
        ),
    ] {
        assert_eq!(ranges(source), expected, "{why}");
    }
    for (why, source) in [
        (
            "negated",
            "from typing import TYPE_CHECKING\nif not TYPE_CHECKING:\n    import a\n",
        ),
        (
            "compound test",
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING or DEBUG:\n    import a\n",
        ),
        (
            "body on the test line",
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING: import a\n",
        ),
        (
            "flag assigned a runtime value",
            "TYPE_CHECKING = os.environ.get('CHECK')\nif TYPE_CHECKING:\n    import a\n",
        ),
        (
            "flag assigned True",
            "TYPE_CHECKING = True\nif TYPE_CHECKING:\n    import a\n",
        ),
        (
            "flag rebound after the typing import",
            "from typing import TYPE_CHECKING\nTYPE_CHECKING = enabled()\nif TYPE_CHECKING:\n    import a\n",
        ),
        (
            "flag from another module",
            "from settings import TYPE_CHECKING\nif TYPE_CHECKING:\n    import a\n",
        ),
        (
            "another name imported as the flag",
            "from typing import Any as TYPE_CHECKING\nif TYPE_CHECKING:\n    import a\n",
        ),
        ("flag never bound", "if TYPE_CHECKING:\n    import a\n"),
        (
            "attribute on a non-typing module",
            "import settings\nif settings.TYPE_CHECKING:\n    import a\n",
        ),
        (
            "typing alias rebound",
            "import typing as t\nt = load()\nif t.TYPE_CHECKING:\n    import a\n",
        ),
        (
            "flag is a parameter",
            "from typing import TYPE_CHECKING\ndef f(TYPE_CHECKING):\n    if TYPE_CHECKING:\n        return 1\n",
        ),
        (
            "star import may bind the flag",
            "from typing import TYPE_CHECKING\nfrom settings import *\nif TYPE_CHECKING:\n    import a\n",
        ),
        (
            "typing attribute patched on",
            "import typing\ntyping.TYPE_CHECKING = True\nif typing.TYPE_CHECKING:\n    import a\n",
        ),
        (
            "flag deleted",
            "from typing import TYPE_CHECKING\ndel TYPE_CHECKING\nif TYPE_CHECKING:\n    import a\n",
        ),
        (
            "does not parse",
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    import (\n",
        ),
    ] {
        assert_eq!(ranges(source), Vec::new(), "{why}");
    }
}
