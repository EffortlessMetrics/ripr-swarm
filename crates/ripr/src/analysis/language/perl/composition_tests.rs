//! Producer owner identity controls; output composition lives under output.

use super::*;

#[test]
fn perl_composition_producer_excludes_other_owner_with_same_name() -> Result<(), String> {
    let text = edited_packet(
        &with_concrete_discriminator(EXACT_RETURN_PACKET.to_string()),
        |packet| {
            let mut owner = packet_array(packet, "owners")?[0].clone();
            owner["owner_id"] = "perl:lib/My/App.pm::Other::discount".into();
            owner["package"] = "Other".into();
            packet_array(packet, "owners")?.push(owner);
            let mut test = packet_array(packet, "tests")?[0].clone();
            test["test_id"] = "test:t/app.t:other_discount".into();
            packet_array(packet, "tests")?.push(test);
            let mut oracle = packet_array(packet, "oracles")?[0].clone();
            oracle["oracle_id"] = "oracle:other:discount".into();
            oracle["test_id"] = "test:t/app.t:other_discount".into();
            oracle["target_owner_id"] = "perl:lib/My/App.pm::Other::discount".into();
            oracle["expression"] = "is(Other::discount(5), 99)".into();
            packet_array(packet, "oracles")?.push(oracle);
            let mut relation = packet_array(packet, "relations")?[0].clone();
            relation["relation_id"] = "relation:other:discount".into();
            relation["owner_id"] = "perl:lib/My/App.pm::Other::discount".into();
            relation["test_id"] = "test:t/app.t:other_discount".into();
            relation["oracle_id"] = "oracle:other:discount".into();
            packet_array(packet, "relations")?.push(relation);
            Ok(())
        },
    )?;
    for text in [text.clone(), reversed_arrays(&text, &all_fact_arrays())?] {
        let finding = only_finding(&text)?;
        assert_eq!(finding.related_tests.len(), 1);
        assert_eq!(
            finding.related_tests[0].oracle.as_deref(),
            Some("is($got, 10, 'discount threshold')")
        );
        assert!(
            !finding.related_tests[0]
                .oracle
                .as_deref()
                .is_some_and(|text| text.contains("Other::discount"))
        );
        // The mapper does not manufacture the complete preview contract.
        assert!(
            !finding
                .evidence
                .iter()
                .any(|line| line.starts_with("perl_stop_if: "))
        );
    }
    Ok(())
}
