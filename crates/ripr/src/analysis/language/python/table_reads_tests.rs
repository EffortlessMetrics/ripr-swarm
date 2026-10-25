use super::{dict_literal_names, unsafe_table_uses};
use std::collections::BTreeSet;

type Check = Result<(), String>;

fn unsafe_in(source: &str, defining_line: Option<usize>) -> BTreeSet<String> {
    unsafe_table_uses(source, &|word| word == "RATES", defining_line)
}

fn assert_safe(source: &str) -> Check {
    let found = unsafe_in(source, Some(1));
    if found.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "expected a safe read in {source:?}, found {found:?}"
        ))
    }
}

fn assert_unsafe(source: &str) -> Check {
    if unsafe_in(source, Some(1)).contains("RATES") {
        Ok(())
    } else {
        Err(format!("expected a possible write in {source:?}"))
    }
}

const DEFINITION: &str = "RATES = {\"gold\": 10}\n";

#[test]
fn definition_imports_and_pure_reads_are_safe() -> Check {
    assert_safe(&format!(
        "{DEFINITION}\ndef price(tier):\n    return RATES.get(tier, 0)\n"
    ))?;
    assert_safe(&format!(
        "{DEFINITION}\ndef price(tier):\n    return RATES[tier]\n"
    ))?;
    assert_safe("from hostile.settings import RATES, price\n\n__all__ = [\"RATES\"]\n")?;
    assert_safe("import hostile.settings\n\nvalue = hostile.settings.RATES.get(\"gold\")\n")?;
    assert_safe("RATES: dict = {\"gold\": 10}\n")
}

#[test]
fn in_place_writes_aliases_and_bare_uses_are_unsafe() -> Check {
    for body in [
        "RATES[\"gold\"] = 1",
        "RATES[\"gold\"] += 1",
        "del RATES[\"gold\"]",
        "RATES.update({\"gold\": 1})",
        "RATES.setdefault(\"x\", 1)",
        "RATES.pop(\"gold\")",
        "RATES |= {\"gold\": 1}",
        "alias = RATES",
        "alias = RATES; alias[\"gold\"] = 1",
        "other = dict(RATES)",
        "for tier in RATES: pass",
        "total = [RATES[t] for t in (1, 2)]",
        "RATES, other = {}, 1",
        "(RATES := {})",
        "print(f\"{RATES.update({})}\")",
        "RATES = {}",
        "global RATES",
        "x = RATES",
    ] {
        assert_unsafe(&format!("{DEFINITION}\ndef run():\n    {body}\n"))?;
    }
    assert_unsafe("from hostile.settings import RATES as table\n")?;
    assert_unsafe("import hostile.settings as settings\nsettings.RATES[\"a\"] = 1\n")
}

#[test]
fn a_second_definition_or_a_lexing_error_is_unsafe() -> Check {
    assert_unsafe(&format!("{DEFINITION}RATES = {{}}\n"))?;
    assert_unsafe("RATES = {\"gold\": 10}\nx = \"unterminated\n")
}

#[test]
fn only_a_foreign_definition_with_no_defining_line_is_unsafe() {
    assert!(unsafe_in(DEFINITION, None).contains("RATES"));
}

#[test]
fn module_scope_dict_literals_are_found() {
    let source = "RATES = {\"a\": 1}\nNAMES: dict = {}\nLIMIT = 3\nx = y = {}\n\ndef f():\n    LOCAL = {}\n\nclass C:\n    FIELD = {}\n";
    let names = dict_literal_names(source);
    assert!(names.contains("RATES"), "{names:?}");
    assert!(names.contains("NAMES"), "{names:?}");
    assert!(names.contains("x"), "{names:?}");
    assert!(!names.contains("LIMIT"), "{names:?}");
    assert!(!names.contains("LOCAL"), "{names:?}");
    assert!(!names.contains("FIELD"), "{names:?}");
}
