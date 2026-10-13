//! Consumer-only guidance controls; authored packets do not establish extraction.

use super::*;

const MORE_PACKET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/perl_cpan_alpha/recommendation-controls/packets/total_more.json"
));
const TEST2_PACKET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/perl_cpan_alpha/recommendation-controls/packets/total_test2.json"
));

fn guidance(finding: &crate::domain::Finding, key: &str) -> Option<String> {
    finding
        .evidence
        .iter()
        .find_map(|line| line.strip_prefix(key).map(str::to_string))
}

#[test]
fn perl_guidance_selects_direct_test_under_packet_permutations() -> Result<(), String> {
    for (base, test) in [(MORE_PACKET, "total_more"), (TEST2_PACKET, "total_test2")] {
        for status in ["complete", "partial"] {
            let text = edited_packet(base, |packet| {
                packet["packet_status"] = status.into();
                Ok(())
            })?;
            let fingerprint = consume(&text)?.packet_fingerprint;
            for arrays in [&[][..], &["relations"][..], &all_fact_arrays()[..]] {
                let reordered = reversed_arrays(&text, arrays)?;
                assert_eq!(consume(&reordered)?.packet_fingerprint, fingerprint);
                let finding = only_finding(&reordered)?;
                assert_eq!(finding.related_tests.len(), 2);
                assert_eq!(finding.class, ExposureClass::WeaklyExposed);
                assert_eq!(finding.canonical_gap.is_some(), status == "complete");
                assert_eq!(
                    guidance(&finding, "perl_suggested_test_location: "),
                    Some(format!("t/{test}.t::line_item_total")),
                    "{test}, {status}, {arrays:?}"
                );
                assert_eq!(
                    guidance(&finding, "perl_verify_command: "),
                    Some(format!("prove -l t/{test}.t")),
                    "location and verification must belong to the same direct test"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn perl_guidance_does_not_borrow_an_advisory_verify_command() -> Result<(), String> {
    let text = edited_packet(MORE_PACKET, |packet| {
        packet_array(packet, "verify_commands")?
            .retain(|command| command["test_id"] == "test:t/load.t:can_total");
        Ok(())
    })?;
    for text in [text.clone(), reversed_arrays(&text, &all_fact_arrays())?] {
        let finding = only_finding(&text)?;
        assert_eq!(
            guidance(&finding, "perl_suggested_test_location: ").as_deref(),
            Some("t/total_more.t::line_item_total")
        );
        assert_eq!(guidance(&finding, "perl_verify_command: "), None);
    }
    Ok(())
}

#[test]
fn perl_guidance_requires_a_direct_reachable_relation() -> Result<(), String> {
    for kind in [
        "package_reference",
        "method_receiver",
        "test_name_match",
        "file_proximity",
        "helper_call",
        "fixture_setup",
        "unknown",
    ] {
        let text = edited_packet(MORE_PACKET, |packet| {
            for relation in packet_array(packet, "relations")? {
                relation["relation_kind"] = kind.into();
            }
            Ok(())
        })?;
        let finding = only_finding(&text)?;
        assert_eq!(
            guidance(&finding, "perl_suggested_test_location: "),
            None,
            "{kind}"
        );
        assert_eq!(
            guidance(&finding, "perl_suggested_assertion: "),
            None,
            "{kind}"
        );
        assert_eq!(guidance(&finding, "perl_verify_command: "), None, "{kind}");
    }
    for hint in ["weakly_reachable", "static_unknown"] {
        let text = edited_packet(MORE_PACKET, |packet| {
            for relation in packet_array(packet, "relations")? {
                relation["reachability_hint"] = hint.into();
            }
            Ok(())
        })?;
        let finding = only_finding(&text)?;
        assert_eq!(
            guidance(&finding, "perl_suggested_test_location: "),
            None,
            "{hint}"
        );
        assert_eq!(guidance(&finding, "perl_verify_command: "), None, "{hint}");
    }
    Ok(())
}

#[test]
fn perl_guidance_prefers_admitted_exact_row_then_stable_test_identity() -> Result<(), String> {
    for strength in ["weak_smoke", "strong_exact"] {
        let mut base = with_second_row(
            &with_concrete_discriminator(EXACT_RETURN_PACKET.to_string()),
            "direct_owner_call",
            Some(("exact_return_assertion", strength)),
            Some("oracle:t/app.t:16:ok"),
        )?;
        if strength == "weak_smoke" {
            base = base.replace(SECOND_TEST, "aaa_smoke");
        }
        for text in [base.clone(), reversed_arrays(&base, &all_fact_arrays())?] {
            let finding = only_finding(&text)?;
            assert_eq!(
                guidance(&finding, "perl_suggested_test_location: ").as_deref(),
                Some("t/app.t::test_discount_threshold"),
                "exact row wins; equal admitted strength uses stable test_id"
            );
            assert_eq!(
                guidance(&finding, "perl_verify_command: ").as_deref(),
                Some("prove t/app.t")
            );
        }
    }
    Ok(())
}

#[test]
fn perl_guidance_verify_choice_is_stable_within_selected_test() -> Result<(), String> {
    let base = edited_packet(MORE_PACKET, |packet| {
        let commands = packet_array(packet, "verify_commands")?;
        let mut alternative = commands
            .iter()
            .find(|command| command["test_id"] == "test:t/total_more.t:line_item_total")
            .cloned()
            .ok_or("missing direct-test command")?;
        alternative["command_id"] = "verify:z-alternative".into();
        alternative["argv"] = serde_json::json!(["prove", "-lv", "t/total_more.t"]);
        commands.insert(0, alternative);
        Ok(())
    })?;
    for text in [base.clone(), reversed_arrays(&base, &all_fact_arrays())?] {
        let finding = only_finding(&text)?;
        assert_eq!(
            guidance(&finding, "perl_verify_command: ").as_deref(),
            Some("prove -l t/total_more.t")
        );
    }
    Ok(())
}

#[test]
fn perl_guidance_real_fixtures_reach_check_and_json() -> Result<(), String> {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/perl_cpan_alpha/recommendation-controls");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("ripr-perl-guidance-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(std::env::temp_dir())
        .map_err(|error| format!("temp parent: {error}"))?;
    std::fs::create_dir(&root).map_err(|error| format!("exclusive fixture root: {error}"))?;
    let result = (|| -> Result<(), String> {
        for path in [
            "lib/Checkout.pm",
            "t/load.t",
            "t/total_more.t",
            "t/total_test2.t",
        ] {
            let destination = root.join(path);
            std::fs::create_dir_all(destination.parent().ok_or("missing parent")?)
                .map_err(|error| format!("fixture directory: {error}"))?;
            std::fs::copy(fixture.join("input").join(path), destination)
                .map_err(|error| format!("copy {path}: {error}"))?;
        }
        let diff = root.join("diff.patch");
        std::fs::copy(fixture.join("diff.patch"), &diff)
            .map_err(|error| format!("copy diff: {error}"))?;
        let config =
            crate::config::tests_only_parse("[languages]\nenabled = [\"rust\", \"perl\"]\n")?;
        for (base, test) in [(MORE_PACKET, "total_more"), (TEST2_PACKET, "total_test2")] {
            for reverse in [false, true] {
                let text = if reverse {
                    reversed_arrays(base, &all_fact_arrays())?
                } else {
                    base.to_string()
                };
                let facts = root.join("facts.json");
                std::fs::write(&facts, &text).map_err(|error| format!("write facts: {error}"))?;
                // On-disk digests and the declared fingerprint must pass ingestion unchanged.
                let output = crate::app::check_workspace_with_config(
                    crate::CheckInput {
                        root: root.clone(),
                        diff_file: Some(diff.clone()),
                        perl_facts_path: Some(facts),
                        ..crate::CheckInput::default()
                    },
                    &config,
                )?;
                assert_eq!(
                    output.findings.len(),
                    1,
                    "must reach the intended nonempty Perl subject"
                );
                assert_eq!(output.findings[0].class, ExposureClass::WeaklyExposed);
                assert_eq!(
                    output.findings[0].source_currentness,
                    crate::domain::SourceCurrentness::CandidateCurrent
                );
                let json = crate::render_check(&output, &crate::OutputFormat::Json)?;
                let value: serde_json::Value = serde_json::from_str(&json)
                    .map_err(|error| format!("rendered JSON: {error}"))?;
                let evidence = value["findings"][0]["evidence"]
                    .as_array()
                    .ok_or("missing evidence array")?;
                for expected in [
                    format!("perl_suggested_test_location: t/{test}.t::line_item_total"),
                    format!("perl_verify_command: prove -l t/{test}.t"),
                ] {
                    assert!(
                        evidence.iter().any(|line| line.as_str() == Some(&expected)),
                        "missing {expected}: {json}"
                    );
                }
                assert!(!evidence.iter().any(|line| line.as_str() == Some("perl_verify_command: prove -l t/load.t")));
            }
        }
        Ok(())
    })();
    // Preserve a failed consumer's fixture for diagnosis; clean only successful owned proof.
    result?;
    std::fs::remove_dir_all(&root).map_err(|error| format!("remove {}: {error}", root.display()))
}
