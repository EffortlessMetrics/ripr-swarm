//! A module constant read through a reader a sibling module's function
//! forwards to across one level-1 relative import (#9619): credited only when
//! the forwarder module's bindings are certain and the pinned assertion flips.

use super::call_chain_tests::{assert_credit, classify};
use crate::domain::{ExposureClass, Finding};

type Check = Result<(), String>;

/// A labelled layout edit and whether it credits.
type Case<'a> = (&'a str, bool, Vec<(&'a str, Option<&'a str>)>);

const TABLE: &str = "RATE_TABLE = {\"gold\": 11, \"silver\": 5}\n\n\ndef lookup(tier):\n    return RATE_TABLE.get(tier, 0)\n";
const QUOTE: &str =
    "from .table import lookup\n\n\ndef quote(tier, units):\n    return lookup(tier) * units\n";
const TEST: &str = "from shapes.rates.quote import quote\n\n\ndef test_quote_gold():\n    assert quote(\"gold\", 3) == 30\n";

/// A scalar owner: `RATE = 10` -> `RATE = 11`.
const SCALAR: &str = "RATE = 11\n\n\ndef lookup(tier):\n    return RATE\n";

/// The table edit `10` -> `11` in the base package with `edits` applied:
/// `Some(text)` writes a file, `None` removes it.
fn table_finding(edits: &[(&str, Option<&str>)]) -> Result<Finding, String> {
    owner_finding(
        TABLE,
        "RATE_TABLE = {\"gold\": 10, \"silver\": 5}",
        "RATE_TABLE = {\"gold\": 11, \"silver\": 5}",
        edits,
    )
}

fn scalar_finding(edits: &[(&str, Option<&str>)]) -> Result<Finding, String> {
    owner_finding(SCALAR, "RATE = 10", "RATE = 11", edits)
}

fn owner_finding(
    owner: &str,
    old: &str,
    new: &str,
    edits: &[(&str, Option<&str>)],
) -> Result<Finding, String> {
    let mut files: Vec<(&str, &str)> = vec![
        ("shapes/__init__.py", ""),
        ("shapes/rates/__init__.py", ""),
        ("shapes/rates/table.py", owner),
        ("shapes/rates/quote.py", QUOTE),
        ("tests/test_rates.py", TEST),
    ];
    for (path, text) in edits {
        files.retain(|(file, _)| file != path);
        if let Some(text) = text {
            files.push((path, text));
        }
    }
    let changed = files
        .iter()
        .find(|(_, text)| text.starts_with(new))
        .map(|(path, _)| *path)
        .ok_or("no changed owner")?;
    classify(&files, changed, 1, old, new)
}

#[test]
fn a_table_read_through_a_relative_import_forwarder_is_credited() -> Check {
    let finding = table_finding(&[])?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(
        finding.evidence.iter().any(|line| line.starts_with(
            "module_value_read: `RATE_TABLE` {\"gold\": 10, \"silver\": 5} -> {\"gold\": 11, \"silver\": 5} through `quote`"
        )),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_forwarder_credits_only_through_certain_bindings_and_a_flipped_assertion() -> Check {
    let quote_as = |body: &str| format!("from .table import lookup\n\n\n{body}");
    let alias = "from .table import lookup as find\n\n\ndef quote(tier, units):\n    return find(tier) * units\n";
    let rebound = format!("{QUOTE}\n\nlookup = len\n");
    let reflects =
        format!("{QUOTE}\n\ndef name(module):\n    return getattr(module, \"lookup\")\n");
    let absolute = QUOTE.replace("from .table", "from shapes.rates.table");
    let keyword = QUOTE.replace("lookup(tier)", "lookup(tier=tier)");
    let decorated = quote_as(
        "def keep(fn):\n    return fn\n\n\n@keep\ndef quote(tier, units):\n    return lookup(tier) * units\n",
    );
    let twice = format!("{QUOTE}\n\ndef quote(tier, units):\n    return 0\n");
    let two_hops = quote_as(
        "def price(tier):\n    return lookup(tier)\n\n\ndef quote(tier, units):\n    return price(tier) * units\n",
    );
    let own_constant = "from .table import RATE_TABLE, lookup\n\n\ndef quote(tier, units):\n    return RATE_TABLE[tier] * units\n";
    // Its own table, not the owner's: `quote("gold", 3)` is 25 on both sides.
    let own_table = "from .table import lookup\n\nRATE_TABLE = {\"gold\": 10}\n\n\ndef quote(tier, units):\n    return lookup(\"silver\") * units + RATE_TABLE[\"gold\"]\n";
    let own_table_test = TEST.replace("== 30", "== 25");
    let nested = "from ..table import lookup\n\n\ndef quote(tier, units):\n    return lookup(tier) * units\n";
    let nested_test = TEST.replace("shapes.rates.quote", "shapes.rates.deep.quote");
    let silver = TEST.replace("quote(\"gold\", 3) == 30", "quote(\"silver\", 3) == 15");
    let by_keyword = TEST.replace("quote(\"gold\", 3)", "quote(\"gold\", units=3)");
    let stdlib_test = TEST.replace("shapes.rates.quote", "json.rates.quote");
    let patcher = "import shapes.rates.quote as forward\n\nforward.find = len\n";
    let level_two = "# from .table import lookup\nfrom ..table import lookup\n\n\ndef quote(tier, units):\n    return lookup(tier) * units\n";
    let base_gone = [
        ("shapes/__init__.py", None),
        ("shapes/rates/__init__.py", None),
        ("shapes/rates/table.py", None),
        ("shapes/rates/quote.py", None),
    ];
    let flat_test = TEST.replace("shapes.rates.quote", "quote");
    let src_package: Vec<(&str, Option<&str>)> = base_gone
        .iter()
        .copied()
        .chain([
            ("src/__init__.py", Some("")),
            ("src/table.py", Some(TABLE)),
            ("src/quote.py", Some(QUOTE)),
            ("tests/test_rates.py", Some(flat_test.as_str())),
        ])
        .collect();
    let src_layout: Vec<(&str, Option<&str>)> = base_gone
        .iter()
        .copied()
        .chain([
            ("src/shapes/__init__.py", Some("")),
            ("src/shapes/rates/__init__.py", Some("")),
            ("src/shapes/rates/table.py", Some(TABLE)),
            ("src/shapes/rates/quote.py", Some(QUOTE)),
        ])
        .collect();
    let mut src_shadowed = src_layout.clone();
    src_shadowed.push(("tests/shapes/__init__.py", Some("")));
    let receiver_test = "import shapes.rates.quote as q\n\n\ndef test_quote_gold():\n    assert q.quote(\"gold\", 3) == 30\n";
    let circular_owner = format!("{TABLE}\n\nfrom .quote import helper\n");
    let circular_quote = format!("{QUOTE}\n\ndef helper():\n    return 1\n");
    let module_call = format!("{QUOTE}\n\nDEFAULT = quote(\"gold\", 1)\n");
    let cases: Vec<Case<'_>> = vec![
        (
            "aliased reader",
            true,
            vec![("shapes/rates/quote.py", Some(alias))],
        ),
        ("src layout", true, src_layout),
        (
            "package imported from inside its own directory",
            false,
            src_package,
        ),
        (
            "test directory package shadows a src-layout package",
            false,
            src_shadowed,
        ),
        (
            "module receiver the package __init__ rebinds",
            false,
            vec![
                (
                    "shapes/rates/__init__.py",
                    Some("from .quote import quote\n"),
                ),
                ("tests/test_rates.py", Some(receiver_test)),
            ],
        ),
        (
            "owner imports the forwarder module",
            false,
            vec![
                ("shapes/rates/table.py", Some(&circular_owner)),
                ("shapes/rates/quote.py", Some(&circular_quote)),
            ],
        ),
        (
            "forwarder module calls the forwarder while loading",
            false,
            vec![("shapes/rates/quote.py", Some(&module_call))],
        ),
        (
            "forwarder module rebinds the reader",
            false,
            vec![("shapes/rates/quote.py", Some(&rebound))],
        ),
        (
            "another file rebinds the forwarder's reader",
            false,
            vec![
                ("shapes/rates/quote.py", Some(alias)),
                ("shapes/rates/patch.py", Some(patcher)),
            ],
        ),
        (
            "forwarder module reflects",
            false,
            vec![("shapes/rates/quote.py", Some(&reflects))],
        ),
        (
            "absolute import of the reader",
            false,
            vec![("shapes/rates/quote.py", Some(&absolute))],
        ),
        (
            "level-2 import from a subpackage",
            false,
            vec![
                ("shapes/rates/quote.py", None),
                ("shapes/rates/deep/__init__.py", Some("")),
                ("shapes/rates/deep/quote.py", Some(nested)),
                ("tests/test_rates.py", Some(&nested_test)),
            ],
        ),
        (
            "level-2 import beside the owner",
            false,
            vec![("shapes/rates/quote.py", Some(level_two))],
        ),
        (
            "keyword call of the reader",
            false,
            vec![("shapes/rates/quote.py", Some(&keyword))],
        ),
        (
            "decorated forwarder",
            false,
            vec![("shapes/rates/quote.py", Some(&decorated))],
        ),
        (
            "forwarder defined twice",
            false,
            vec![("shapes/rates/quote.py", Some(&twice))],
        ),
        (
            "two forwarding hops",
            false,
            vec![("shapes/rates/quote.py", Some(&two_hops))],
        ),
        (
            "forwarder reads the constant itself",
            false,
            vec![("shapes/rates/quote.py", Some(own_constant))],
        ),
        (
            "forwarder reads its own same-named table",
            false,
            vec![
                ("shapes/rates/quote.py", Some(own_table)),
                ("tests/test_rates.py", Some(&own_table_test)),
            ],
        ),
        (
            "same-named module elsewhere",
            false,
            vec![("quote.py", Some("VALUE = 1\n"))],
        ),
        (
            "extension beside the forwarder",
            false,
            vec![(
                "shapes/rates/quote.cpython-313-x86_64-linux-gnu.so",
                Some(""),
            )],
        ),
        (
            "test directory package shadows the top-level package",
            false,
            vec![("tests/shapes/__init__.py", Some(""))],
        ),
        (
            "test directory module shadows the top-level package",
            false,
            vec![("tests/shapes.py", Some(""))],
        ),
        (
            "namespace package",
            false,
            vec![("shapes/rates/__init__.py", None)],
        ),
        (
            "assertion the edit does not move",
            false,
            vec![("tests/test_rates.py", Some(&silver))],
        ),
        (
            "keyword argument in the test",
            false,
            vec![("tests/test_rates.py", Some(&by_keyword))],
        ),
        (
            "package named for a standard-library module",
            false,
            vec![
                ("shapes/__init__.py", None),
                ("shapes/rates/__init__.py", None),
                ("shapes/rates/table.py", None),
                ("shapes/rates/quote.py", None),
                ("json/__init__.py", Some("")),
                ("json/rates/__init__.py", Some("")),
                ("json/rates/table.py", Some(TABLE)),
                ("json/rates/quote.py", Some(QUOTE)),
                ("tests/test_rates.py", Some(&stdlib_test)),
            ],
        ),
    ];
    for (label, credited, edits) in cases {
        let finding = table_finding(&edits)?;
        assert_credit(&finding, credited, label);
    }
    Ok(())
}

#[test]
fn a_scalar_constant_forwarder_credits_only_from_a_certain_module() -> Check {
    let reflects = format!("{QUOTE}\n\ndef name(module):\n    return getattr(module, \"quote\")\n");
    // `RATE` is unbound in the forwarder's module: the call raises on both
    // sides.
    let own_rate = "from .table import lookup\n\n\ndef quote(tier, units):\n    return lookup(tier) * 0 + RATE\n";
    let own_rate_test = TEST.replace("== 30", "== 10");
    let cases: Vec<Case<'_>> = vec![
        ("scalar through the forwarder", true, Vec::new()),
        (
            "forwarder module reflects",
            false,
            vec![("shapes/rates/quote.py", Some(&reflects))],
        ),
        (
            "forwarder names the owner's constant unbound",
            false,
            vec![
                ("shapes/rates/quote.py", Some(own_rate)),
                ("tests/test_rates.py", Some(&own_rate_test)),
            ],
        ),
    ];
    for (label, credited, edits) in cases {
        let finding = scalar_finding(&edits)?;
        assert_credit(&finding, credited, label);
    }
    Ok(())
}
