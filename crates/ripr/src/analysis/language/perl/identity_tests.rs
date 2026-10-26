use super::*;

fn packet() -> Result<PerlFactPacket, String> {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/perl_packet_contract_migration/sarif_identity/current_two/packet.json"
    ));
    PerlAdapter.consume_fact_packet(text, &super::tests::packet_test_options())
}

fn findings(packet: &mut PerlFactPacket) -> Result<Vec<crate::domain::Finding>, String> {
    packet.reset_index();
    packet.packet_fingerprint = packet.recompute_packet_fingerprint();
    packet.validate_ingestion(&super::tests::packet_test_options())?;
    let findings = packet_to_findings(packet);
    assert_eq!(
        findings.len(),
        packet.changes.len(),
        "all intended changes must project"
    );
    assert!(
        findings
            .iter()
            .all(|finding| finding.id == finding.probe.id.0)
    );
    Ok(findings)
}

fn ids(packet: &mut PerlFactPacket) -> Result<BTreeMap<String, String>, String> {
    let findings = findings(packet)?;
    Ok(packet
        .changes
        .iter()
        .zip(findings)
        .map(|(change, finding)| (change.change_id.clone(), finding.id))
        .collect())
}

#[test]
fn perl_identity_uses_named_owner_and_change_payload_without_coordinates() -> Result<(), String> {
    let mut original = packet()?;
    let original_ids = findings(&mut original)?
        .into_iter()
        .map(|f| f.id)
        .collect::<Vec<_>>();
    assert_ne!(
        original_ids[0], original_ids[1],
        "same owner/hint retains distinct changes"
    );

    let mut moved = original.clone();
    moved.owners[0].owner_id.push_str(":moved");
    moved.owners[0].range.start_line += 4;
    moved.owners[0].range.end_line += 4;
    for change in &mut moved.changes {
        change.change_id.push_str(":moved");
        change.owner_id = moved.owners[0].owner_id.clone();
        change.range.start_line += 6;
        change.range.end_line += 6;
    }
    assert_eq!(
        findings(&mut moved)?
            .into_iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        original_ids,
        "volatile packet tokens and geometry cannot rekey a named semantic payload"
    );
    Ok(())
}

#[test]
fn perl_identity_distinguishes_material_payload_and_observable_states() -> Result<(), String> {
    let mut original = packet()?;
    original.changes.truncate(1);
    let base = findings(&mut original)?.remove(0).id;
    for observable in [
        ObservableField(None),
        ObservableField(Some(None)),
        ObservableField(Some(Some(String::new()))),
    ] {
        let mut changed = original.clone();
        changed.changes[0].changed_observable = observable;
        assert_ne!(findings(&mut changed)?.remove(0).id, base);
    }
    let mut states = BTreeSet::new();
    for observable in [
        ObservableField(None),
        ObservableField(Some(None)),
        ObservableField(Some(Some(String::new()))),
    ] {
        let mut changed = original.clone();
        changed.changes[0].changed_observable = observable;
        states.insert(findings(&mut changed)?.remove(0).id);
    }
    assert_eq!(
        states.len(),
        3,
        "missing, explicit null and empty value are different facts"
    );
    let mut changed = original.clone();
    changed.changes[0].changed_text_digest.push_str(":changed");
    assert_ne!(findings(&mut changed)?.remove(0).id, base);
    let mut changed = original.clone();
    changed.changes[0].behavior_hint = BehaviorHint::PredicateBoundary;
    assert_ne!(findings(&mut changed)?.remove(0).id, base);
    Ok(())
}

#[test]
fn perl_identity_duplicate_payload_ordinals_follow_source_order_not_packet_order()
-> Result<(), String> {
    let mut original = packet()?;
    original.changes[1] = original.changes[0].clone();
    original.changes[1].change_id.push_str(":second");
    original.changes[1].range.start_column += 4;
    original.changes[1].range.end_column += 4;
    let expected = ids(&mut original)?;
    assert_eq!(
        expected.values().collect::<BTreeSet<_>>().len(),
        2,
        "same-line changes remain separate"
    );
    original.changes.reverse();
    assert_eq!(ids(&mut original)?, expected);
    for change in &mut original.changes {
        change.range.start_line += 2;
        change.range.end_line += 2;
    }
    assert_eq!(
        ids(&mut original)?,
        expected,
        "same duplicate set and source order survives a shift"
    );
    Ok(())
}

#[test]
fn perl_identity_falls_back_for_missing_or_ambiguous_owner_facts() -> Result<(), String> {
    for case in [
        "name_missing",
        "name_blank",
        "package_missing",
        "package_blank",
        "kind_unknown",
        "digest_blank",
        "duplicate_owner",
        "owner_file_mismatch",
    ] {
        let mut original = packet()?;
        match case {
            "name_missing" => original.owners[0].name = None,
            "name_blank" => original.owners[0].name = Some(" ".to_string()),
            "package_missing" => original.owners[0].package = None,
            "package_blank" => original.owners[0].package = Some(" ".to_string()),
            "kind_unknown" => original.owners[0].kind = OwnerKind::Unknown,
            "digest_blank" => original.changes[0].changed_text_digest = " ".to_string(),
            "duplicate_owner" => {
                let mut duplicate = original.owners[0].clone();
                duplicate.owner_id.push_str(":another-definition");
                duplicate.range.start_line += 20;
                original.owners.push(duplicate);
            }
            "owner_file_mismatch" => {
                let mut file = original.files[0].clone();
                file.file_id = "file:lib/Other.pm".to_string();
                file.path = "lib/Other.pm".to_string();
                original.files.push(file);
                original.owners[0].file_id = "file:lib/Other.pm".to_string();
            }
            _ => return Err("unknown fallback fixture".to_string()),
        }
        original.changes[1].changed_text_digest = original.changes[0].changed_text_digest.clone();
        original.changes[1].changed_observable = original.changes[0].changed_observable.clone();
        original.changes[1].range.start_line = original.changes[0].range.start_line;
        original.changes[1].range.end_line = original.changes[0].range.end_line;
        original.changes[1].range.start_column = original.changes[0].range.start_column + 4;
        original.changes[1].range.end_column = original.changes[0].range.end_column + 4;
        let before = ids(&mut original)?;
        assert_eq!(before.values().collect::<BTreeSet<_>>().len(), 2);
        assert!(
            before
                .values()
                .all(|id| id.starts_with("probe:perl-location:"))
        );
        original.owners[0].range.start_line += 2;
        assert_ne!(
            ids(&mut original)?,
            before,
            "fallback makes no line-stability promise"
        );
    }
    Ok(())
}

#[test]
fn perl_identity_keeps_files_and_framed_owner_fields_distinct() -> Result<(), String> {
    let mut original = packet()?;
    original.changes[1].changed_text_digest = original.changes[0].changed_text_digest.clone();
    original.changes[1].changed_observable = original.changes[0].changed_observable.clone();
    let mut other_file = original.files[0].clone();
    other_file.file_id = "file:lib/Other.pm".to_string();
    other_file.path = "lib/Other.pm".to_string();
    original.files.push(other_file);
    let mut other_owner = original.owners[0].clone();
    other_owner.owner_id = "owner:other".to_string();
    other_owner.file_id = "file:lib/Other.pm".to_string();
    original.owners.push(other_owner);
    original.changes[1].file_id = "file:lib/Other.pm".to_string();
    original.changes[1].owner_id = "owner:other".to_string();
    let projected = findings(&mut original)?;
    assert_ne!(projected[0].id, projected[1].id);

    let mut left = packet()?;
    left.changes.truncate(1);
    left.owners[0].package = Some("A\0B".to_string());
    left.owners[0].name = Some("C".to_string());
    let mut right = left.clone();
    right.owners[0].package = Some("A".to_string());
    right.owners[0].name = Some("B\0C".to_string());
    assert_ne!(
        findings(&mut left)?[0].id,
        findings(&mut right)?[0].id,
        "field framing cannot merge owner tuples"
    );
    Ok(())
}
