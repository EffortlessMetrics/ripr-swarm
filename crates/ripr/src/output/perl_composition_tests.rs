//! Declared Perl tuple composition, not producer extraction or packet readiness.

use crate::domain::{DiagnosticWitness, ExposureClass, Finding, OracleKind, OracleStrength};
use crate::output::perl_gap_record_projection::perl_gap_record_for;
use crate::output::perl_preview_card::{perl_preview_card, tests::sample_perl_finding};

const GAP: &str =
    "gap:perl:lib/My/App.pm:My::App::discount:return_value:exact_return_assertion:return_value";

/// Complete synthetic consumer contract; real sparse packets stay separate.
fn declared_tuple(advisory_a: bool) -> Finding {
    let mut finding = sample_perl_finding();
    let row = finding.related_tests[0].clone();
    let mut a = row.clone();
    a.file = "t/a.t".into();
    a.name = "a_exact".to_string();
    a.line = 11;
    a.oracle = Some("is($got, 11, 'A exact')".to_string());
    a.oracle_kind = OracleKind::ExactValue;
    a.oracle_strength = OracleStrength::Strong;
    let mut b = row;
    b.file = "t/b.t".into();
    b.name = "b_exact".to_string();
    b.line = 22;
    b.oracle = Some("is($got, 22, 'B exact')".to_string());
    b.oracle_kind = OracleKind::ExactValue;
    b.oracle_strength = OracleStrength::Strong;
    if advisory_a {
        a.oracle = Some("ok($got, 'A smoke')".to_string());
        a.oracle_kind = OracleKind::SmokeOnly;
        a.oracle_strength = OracleStrength::Weak;
    }
    finding.related_tests = vec![a, b];
    finding.evidence.retain(|line| {
        !line.starts_with("perl_suggested_test_location: ")
            && !line.starts_with("perl_suggested_assertion: ")
            && !line.starts_with("perl_verify_command: ")
            && !line.starts_with("perl_current_test_evidence: ")
    });
    let (file, name, assertion) = if advisory_a {
        ("t/b.t", "b_exact", "is($got, 22, 'B exact')")
    } else {
        ("t/a.t", "a_exact", "is($got, 11, 'A exact')")
    };
    finding.evidence.extend([
        format!("perl_suggested_test_location: {file}::{name}"),
        format!("perl_suggested_assertion: {assertion}"),
        format!("perl_verify_command: prove {file}"),
    ]);
    finding
}

#[test]
fn perl_composition_witness_preserves_declared_tuple() -> Result<(), String> {
    for advisory_a in [false, true] {
        for reversed in [false, true] {
            let mut finding = declared_tuple(advisory_a);
            if reversed {
                finding.related_tests.reverse();
            }
            let witness = DiagnosticWitness::from_finding(&finding).ok_or("missing witness")?;
            let site = witness.fix_site.ok_or("missing fix site")?;
            let expected = if advisory_a {
                ("t/b.t", "b_exact", 22, "is($got, 22, 'B exact')")
            } else {
                ("t/a.t", "a_exact", 11, "is($got, 11, 'A exact')")
            };
            assert_eq!(
                (
                    site.file.as_str(),
                    site.test_name.as_str(),
                    site.line,
                    site.current_oracle.as_deref()
                ),
                (expected.0, expected.1, expected.2, Some(expected.3))
            );
            // Current oracle text is not a symbol-resolved new assertion template.
            assert!(witness.suggested_assertion.is_none());
        }
    }
    Ok(())
}

#[test]
fn perl_composition_gap_projection_preserves_declared_tuple() -> Result<(), String> {
    for advisory_a in [false, true] {
        for reversed in [false, true] {
            let mut finding = declared_tuple(advisory_a);
            if reversed {
                finding.related_tests.reverse();
            }
            let record =
                perl_gap_record_for(&finding).ok_or("complete synthetic projection missing")?;
            assert_eq!(record.canonical_gap_id, GAP);
            let route = record.repair_route.ok_or("missing projected route")?;
            let (file, name, line, assertion) = if advisory_a {
                ("t/b.t", "t/b.t::b_exact", 22, "is($got, 22, 'B exact')")
            } else {
                ("t/a.t", "t/a.t::a_exact", 11, "is($got, 11, 'A exact')")
            };
            assert_eq!(route.target_file.as_deref(), Some(file));
            assert_eq!(route.related_test.as_deref(), Some(name));
            assert_eq!(route.target_line, Some(line));
            assert_eq!(route.assertion_shape.as_deref(), Some(assertion));
            assert_eq!(record.verification_commands, [format!("prove {file}")]);
            assert_eq!(record.authority_boundary, "preview_advisory_only");
            let mut sparse = finding;
            sparse
                .evidence
                .retain(|line| !line.starts_with("perl_stop_if: "));
            assert!(perl_gap_record_for(&sparse).is_none());
        }
    }
    Ok(())
}

#[test]
fn perl_composition_preview_keeps_declared_current_oracle() -> Result<(), String> {
    for reversed in [false, true] {
        let mut finding = declared_tuple(false);
        if reversed {
            finding.related_tests.reverse();
        }
        let card = perl_preview_card(&finding).ok_or("missing synthetic preview card")?;
        assert_eq!(card.suggested_test_location, "t/a.t::a_exact");
        assert_eq!(card.suggested_assertion, "is($got, 11, 'A exact')");
        assert_eq!(card.verify_command, "prove t/a.t");
        assert!(card.current_test_evidence.contains("t/a.t:11 a_exact"));
        assert!(
            card.current_test_evidence
                .contains("is($got, 11, 'A exact')")
        );
        assert!(!card.current_test_evidence.contains("B exact"));
        assert!(!card.repair_packet_ready && !card.agent_packet_ready && !card.gate_candidate);
    }
    Ok(())
}

#[test]
fn perl_composition_unresolved_or_ambiguous_tuple_does_not_rank_another_test() -> Result<(), String>
{
    for mode in ["missing", "unknown", "ambiguous"] {
        let mut finding = declared_tuple(false);
        if mode == "missing" {
            finding
                .evidence
                .retain(|line| !line.starts_with("perl_suggested_test_location: "));
        } else if mode == "unknown" {
            for line in &mut finding.evidence {
                if line.starts_with("perl_suggested_test_location: ") {
                    *line = "perl_suggested_test_location: t/other.t::a_exact".to_string();
                }
            }
        } else {
            finding.related_tests.push(finding.related_tests[0].clone());
        }
        let witness = DiagnosticWitness::from_finding(&finding).ok_or("missing limited witness")?;
        assert!(witness.fix_site.is_none(), "{mode}");
        assert!(perl_gap_record_for(&finding).is_none(), "{mode}");
        assert!(perl_preview_card(&finding).is_none(), "{mode}");
    }
    Ok(())
}

fn exact_name_tuple(file: &str, declaration_file: &str, name: &str) -> Finding {
    let mut finding = declared_tuple(false);
    finding.related_tests[0].file = file.into();
    finding.related_tests[0].name = name.to_string();
    finding.related_tests[1].file = file.into();
    finding.related_tests[1].name = name.replace('\\', "/");
    for line in &mut finding.evidence {
        if line.starts_with("perl_suggested_test_location: ") {
            *line = format!("perl_suggested_test_location: {declaration_file}::{name}");
        } else if line.starts_with("perl_verify_command: ") {
            *line = format!("perl_verify_command: prove {file}");
        }
    }
    finding
}

fn assert_exact_name_tuple(mut finding: Finding, file: &str, name: &str) -> Result<(), String> {
    for reversed in [false, true] {
        if reversed {
            finding.related_tests.reverse();
        }
        let witness = DiagnosticWitness::from_finding(&finding).ok_or("missing witness")?;
        let site = witness.fix_site.ok_or("missing exact-name fix site")?;
        assert_eq!(site.file, file);
        assert_eq!(site.test_name, name);
        assert_eq!(site.line, 11);
        assert_eq!(
            site.current_oracle.as_deref(),
            Some("is($got, 11, 'A exact')")
        );
        let record = perl_gap_record_for(&finding).ok_or("missing exact-name projection")?;
        assert_eq!(record.canonical_gap_id, GAP);
        assert_eq!(record.verification_commands, [format!("prove {file}")]);
        let route = record.repair_route.ok_or("missing route")?;
        assert_eq!(route.target_file.as_deref(), Some(file));
        assert_eq!(route.related_test, Some(format!("{file}::{name}")));
        assert_eq!(route.target_line, Some(11));
        assert_eq!(
            route.assertion_shape.as_deref(),
            Some("is($got, 11, 'A exact')")
        );
        let card = perl_preview_card(&finding).ok_or("missing exact-name card")?;
        assert_eq!(card.suggested_test_location, format!("{file}::{name}"));
        assert_eq!(card.verify_command, format!("prove {file}"));
        assert!(
            card.current_test_evidence
                .contains(&format!("{file}:11 {name}"))
        );
        assert!(
            card.current_test_evidence
                .contains("is($got, 11, 'A exact')")
        );
        assert!(!card.current_test_evidence.contains("B exact"));
        assert!(!card.repair_packet_ready && !card.agent_packet_ready && !card.gate_candidate);
    }
    Ok(())
}

#[test]
fn perl_composition_names_are_exact_in_same_file() -> Result<(), String> {
    // Test descriptions are identities: these two realistic path labels differ.
    assert_exact_name_tuple(
        exact_name_tuple("t/same.t", "t/same.t", r"price\discount"),
        "t/same.t",
        r"price\discount",
    )
}

#[test]
fn perl_composition_normalizes_only_file_with_qualified_test_name() -> Result<(), String> {
    assert_exact_name_tuple(
        exact_name_tuple(
            "t/nested/case.t",
            r"t\nested\case.t",
            r"outer::price\discount",
        ),
        "t/nested/case.t",
        r"outer::price\discount",
    )
}

#[test]
fn perl_composition_test_name_whitespace_is_identity() -> Result<(), String> {
    let mut finding = exact_name_tuple("t/same.t", "t/same.t", "price ");
    finding.related_tests[1].name = "price".to_string();
    assert_exact_name_tuple(finding.clone(), "t/same.t", "price ")?;
    let card = perl_preview_card(&finding).ok_or("missing whitespace-name card")?;
    assert_eq!(card.suggested_test_location, "t/same.t::price ");
    Ok(())
}

#[test]
fn perl_composition_real_sparse_packet_stays_advisory() -> Result<(), String> {
    use crate::analysis::{EXACT_RETURN_PACKET, only_finding, with_concrete_discriminator};
    let finding = only_finding(&with_concrete_discriminator(
        EXACT_RETURN_PACKET.to_string(),
    ))?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(finding.canonical_gap.is_some());
    assert!(!finding.related_tests.is_empty());
    assert!(DiagnosticWitness::from_finding(&finding).is_some());
    assert!(perl_gap_record_for(&finding).is_none());
    assert!(perl_preview_card(&finding).is_none());
    Ok(())
}
