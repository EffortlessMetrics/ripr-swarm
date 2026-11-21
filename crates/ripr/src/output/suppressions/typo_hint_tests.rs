//! #15252: manifest typos name the one real mistake and the nearest valid
//! spelling, and an unknown table header does not cascade.

use super::{ENTRY_FIELDS, SUPPRESSIONS_PATH, parse_suppressions_manifest};

const ENTRY_BODY: &str =
    "kind = \"exposure_gap\"\npath = \"src/lib.rs\"\nreason = \"r\"\nowner = \"o\"\n";

fn violations_for(text: &str) -> Vec<String> {
    parse_suppressions_manifest(text).1
}

#[test]
fn misspelled_entry_header_is_one_header_violation_with_a_hint() {
    let violations = violations_for(&format!(
        "schema_version = 1\n[[suppression]]\n{ENTRY_BODY}"
    ));
    assert_eq!(
        violations,
        vec![format!(
            "{SUPPRESSIONS_PATH}:2 unsupported table header `[[suppression]]`; each entry starts with `[[suppressions]]` (did you mean `[[suppressions]]`?)"
        )],
    );
}

#[test]
fn single_bracket_header_names_the_array_header() {
    let violations = violations_for(&format!("schema_version = 1\n[suppressions]\n{ENTRY_BODY}"));
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("unsupported table header `[suppressions]`")
            && violations[0].ends_with("(did you mean `[[suppressions]]`?)"),
        "{violations:?}"
    );
}

#[test]
fn unrelated_header_fails_without_a_hint_and_still_rejects_the_manifest() {
    let text =
        format!("schema_version = 1\n[[suppressions]]\n{ENTRY_BODY}[metadata]\nnote = \"x\"\n");
    let (entries, violations) = parse_suppressions_manifest(&text);
    assert_eq!(entries.len(), 1, "the entry before the header still parses");
    assert_eq!(
        violations,
        vec![format!(
            "{SUPPRESSIONS_PATH}:7 unsupported table header `[metadata]`; each entry starts with `[[suppressions]]`"
        )],
    );
}

#[test]
fn a_valid_header_after_an_unknown_table_resumes_normal_checks() {
    let text = format!(
        "schema_version = 1\n[meta]\nnote = \"x\"\n[[suppressions]]\n{ENTRY_BODY}functon = \"f\"\n"
    );
    let violations = violations_for(&text);
    assert_eq!(violations.len(), 2, "{violations:?}");
    assert!(violations[0].contains(":2 unsupported table header `[meta]`"));
    assert!(violations[1].contains(":9 unsupported `[[suppressions]]` field `functon`"));
}

#[test]
fn toml_header_spellings_of_the_entry_header_are_accepted() {
    for header in [
        "[[ suppressions ]]",
        "[[suppressions]] # accepted debt",
        "[[suppressions]]#x",
    ] {
        let (entries, violations) =
            parse_suppressions_manifest(&format!("schema_version = 1\n{header}\n{ENTRY_BODY}"));
        assert!(violations.is_empty(), "{header}: {violations:?}");
        assert_eq!(entries.len(), 1, "{header}");
    }
}

#[test]
fn trailing_text_after_the_entry_header_is_rejected() {
    let violations = violations_for(&format!(
        "schema_version = 1\n[[suppressions]] extra\n{ENTRY_BODY}"
    ));
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("unsupported table header `[[suppressions]] extra`"));
}

#[test]
fn misspelled_entry_fields_name_the_nearest_field() {
    for (typo, nearest) in [
        ("functon", "function"),
        ("langauge", "language"),
        ("expries", "expires"),
    ] {
        let violations = violations_for(&format!(
            "schema_version = 1\n[[suppressions]]\n{ENTRY_BODY}{typo} = \"x\"\n"
        ));
        assert_eq!(
            violations,
            vec![format!(
                "{SUPPRESSIONS_PATH}:7 unsupported `[[suppressions]]` field `{typo}` (did you mean `{nearest}`?)"
            )],
        );
    }
}

#[test]
fn an_unrelated_entry_field_gets_no_hint() {
    let violations = violations_for(&format!(
        "schema_version = 1\n[[suppressions]]\n{ENTRY_BODY}priority = \"high\"\n"
    ));
    assert_eq!(
        violations,
        vec![format!(
            "{SUPPRESSIONS_PATH}:7 unsupported `[[suppressions]]` field `priority`"
        )],
    );
}

#[test]
fn an_entry_field_before_any_header_says_where_it_belongs() {
    let violations =
        violations_for("schema_version = 1\nkind = \"exposure_gap\"\nschema_verison = 1\n");
    assert_eq!(
        violations,
        vec![
            format!(
                "{SUPPRESSIONS_PATH}:2 unsupported top-level field `kind`; it belongs inside a `[[suppressions]]` entry"
            ),
            format!(
                "{SUPPRESSIONS_PATH}:3 unsupported top-level field `schema_verison` (did you mean `schema_version`?)"
            ),
        ],
    );
}

#[test]
fn misspelled_kind_names_the_nearest_kind() {
    let violations = violations_for(
        "schema_version = 1\n[[suppressions]]\nkind = \"exposure-gap\"\npath = \"src/lib.rs\"\nreason = \"r\"\nowner = \"o\"\n",
    );
    assert_eq!(
        violations,
        vec![format!(
            "{SUPPRESSIONS_PATH}:3 unsupported kind `exposure-gap`; supported: exposure_gap, test_efficiency (did you mean `exposure_gap`?)"
        )],
    );
}

#[test]
fn every_listed_entry_field_is_one_the_parser_accepts() {
    for field in ENTRY_FIELDS {
        let violations = violations_for(&format!(
            "schema_version = 1\n[[suppressions]]\n{field} = \"x\"\n"
        ));
        assert!(
            !violations
                .iter()
                .any(|v| v.contains(&format!("field `{field}`"))),
            "{field}: {violations:?}"
        );
    }
}
