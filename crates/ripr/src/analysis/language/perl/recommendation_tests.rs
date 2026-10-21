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
fn perl_guidance_review_empty_command_cannot_hide_usable_same_test_command() -> Result<(), String> {
    for (base, test) in [(MORE_PACKET, "total_more"), (TEST2_PACKET, "total_test2")] {
        let text = edited_packet(base, |packet| {
            let commands = packet_array(packet, "verify_commands")?;
            let mut empty = commands
                .iter()
                .find(|command| command["test_id"] == format!("test:t/{test}.t:line_item_total"))
                .cloned()
                .ok_or("missing direct-test command")?;
            empty["command_id"] = "verify:a-empty".into();
            empty["argv"] = serde_json::json!([]);
            commands.insert(0, empty);
            Ok(())
        })?;
        for text in [text.clone(), reversed_arrays(&text, &all_fact_arrays())?] {
            let finding = only_finding(&text)?;
            assert_eq!(finding.class, ExposureClass::WeaklyExposed);
            assert_eq!(
                guidance(&finding, "perl_verify_command: "),
                Some(format!("prove -l t/{test}.t")),
                "empty lower-ID command must not hide usable same-test verification"
            );
            let empty_only = edited_packet(&text, |packet| {
                packet_array(packet, "verify_commands")?.retain(|command| {
                    command["test_id"] == "test:t/load.t:can_total"
                        || command["command_id"] == "verify:a-empty"
                });
                Ok(())
            })?;
            assert_eq!(
                guidance(&only_finding(&empty_only)?, "perl_verify_command: "),
                None,
                "empty selected-test commands cannot borrow valid advisory verification"
            );
        }
    }
    Ok(())
}

#[test]
fn perl_guidance_review_strict_actionability_keeps_usable_command_and_reference()
-> Result<(), String> {
    let fixture = include_str!(
        "../../../../../../fixtures/perl_lsp_facts_exporter/expected/ripr-perl-source-test-oracle-facts-v1.json"
    );
    let mut base = consume(fixture)?;
    let test_id = "test:t/app.t:discount_smoke";
    let change_id = "change:lib/My/App.pm:8:return";
    let mut empty = base
        .verify_command_for_test(test_id)
        .cloned()
        .ok_or("missing selected-test command")?;
    empty.command_id = "verify:a-empty".to_string();
    empty.argv.clear();
    base.verify_commands.insert(0, empty);
    let context = complete_perl_actionability_context();
    for reverse in [false, true] {
        let mut packet = base.clone();
        if reverse {
            packet.verify_commands.reverse();
        }
        packet.validate_ingestion(&packet_test_options())?;
        let actionable = packet
            .strict_actionability_for_change(change_id, &context)
            .map_err(|error| format!("usable same-test verification rejected: {error:?}"))?;
        assert_eq!(actionable.verify_command, ["prove", "t/app.t"]);
        assert!(actionable.raw_evidence_refs.iter().any(|reference| {
            reference.kind == "perl_verify_command"
                && reference.source_id == "verify:t/app.t:prove"
                && reference.path == "t/app.t"
        }));
        packet.verify_commands.retain(|command| {
            command.test_id.as_deref() != Some(test_id) || command.argv.is_empty()
        });
        packet.reset_index();
        assert_eq!(
            packet.strict_actionability_for_change(change_id, &context),
            Err(PerlActionabilityBlocker::MissingVerifyCommand)
        );
    }
    Ok(())
}

#[test]
fn perl_guidance_review_mixed_oracles_keep_canonical_gap_and_probe_stable() -> Result<(), String> {
    for (base, test) in [(MORE_PACKET, "total_more"), (TEST2_PACKET, "total_test2")] {
        let text = edited_packet(base, |packet| {
            let mut smoke = packet_array(packet, "oracles")?
                .first()
                .cloned()
                .ok_or("missing direct oracle")?;
            smoke["oracle_id"] = "oracle:load:smoke".into();
            smoke["test_id"] = "test:t/load.t:can_total".into();
            smoke["kind"] = "smoke_ok".into();
            smoke["strength"] = "weak_smoke".into();
            smoke["expression"] = "can_ok('Checkout', 'total_cents')".into();
            smoke["range"] = serde_json::json!({
                "start_line": 6, "start_column": 1, "end_line": 6, "end_column": 35
            });
            smoke["provenance_refs"] = serde_json::json!(["prov:load"]);
            packet_array(packet, "oracles")?.push(smoke);
            packet_array(packet, "relations")?
                .iter_mut()
                .find(|relation| relation["relation_kind"] == "package_reference")
                .ok_or("missing advisory relation")?["oracle_id"] = "oracle:load:smoke".into();
            Ok(())
        })?;
        let fingerprint = consume(&bless_fingerprint(&text))?.packet_fingerprint;
        for text in [text.clone(), reversed_arrays(&text, &all_fact_arrays())?] {
            assert_eq!(
                consume(&bless_fingerprint(&text))?.packet_fingerprint,
                fingerprint
            );
            let finding = only_finding(&text)?;
            assert_eq!(finding.related_tests.len(), 2);
            assert_eq!(finding.class, ExposureClass::WeaklyExposed);
            assert_eq!(
                guidance(&finding, "perl_suggested_test_location: "),
                Some(format!("t/{test}.t::line_item_total"))
            );
            assert_eq!(
                guidance(&finding, "perl_verify_command: "),
                Some(format!("prove -l t/{test}.t"))
            );
            let gap = finding.canonical_gap.ok_or("missing canonical gap")?;
            assert_eq!(gap.id, "gap:perl:43741247fd8b9df5");
            assert_eq!(gap.probe_kind, "exact_return_assertion");
            assert_eq!(finding.probe.required_oracles, ["exact_return_assertion"]);
            assert_eq!(finding.probe.expected_sinks, ["exact_return_assertion"]);
        }
    }
    Ok(())
}

#[test]
fn perl_guidance_review_shape_requires_selected_test_and_owner_binding() -> Result<(), String> {
    for binding in [
        "bound",
        "foreign_test",
        "foreign_owner",
        "absent",
        "advisory_only",
    ] {
        let text = edited_packet(MORE_PACKET, |packet| {
            let oracle = packet_array(packet, "oracles")?
                .first_mut()
                .ok_or("missing oracle")?;
            oracle["kind"] = "output_observer".into();
            if binding == "foreign_test" {
                oracle["test_id"] = "test:t/load.t:can_total".into();
            } else if binding == "foreign_owner" {
                let mut foreign = packet_array(packet, "owners")?
                    .first()
                    .cloned()
                    .ok_or("missing owner")?;
                foreign["owner_id"] = "perl:lib/Checkout.pm::Checkout::other".into();
                foreign["name"] = "other".into();
                packet_array(packet, "owners")?.push(foreign);
                packet_array(packet, "oracles")?[0]["target_owner_id"] =
                    "perl:lib/Checkout.pm::Checkout::other".into();
            } else if binding == "absent" {
                for relation in packet_array(packet, "relations")? {
                    relation["oracle_id"] = serde_json::Value::Null;
                }
            } else if binding == "advisory_only" {
                for relation in packet_array(packet, "relations")? {
                    relation["relation_kind"] = "package_reference".into();
                }
            }
            Ok(())
        })?;
        for text in [text.clone(), reversed_arrays(&text, &all_fact_arrays())?] {
            let finding = only_finding(&text)?;
            let expected = if binding == "bound" {
                "output_observer"
            } else {
                "exact_return_assertion"
            };
            let gap = finding.canonical_gap.ok_or("missing canonical gap")?;
            assert_eq!(gap.probe_kind, expected, "{binding}");
            assert_eq!(finding.probe.required_oracles, [expected], "{binding}");
            assert_eq!(finding.probe.expected_sinks, [expected], "{binding}");
            if binding != "bound" {
                assert_eq!(gap.id, "gap:perl:43741247fd8b9df5", "{binding}");
            }
        }
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
