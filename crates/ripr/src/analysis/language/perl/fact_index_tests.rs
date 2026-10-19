//! Regression tests for the keyed packet index (quadratic Perl analysis).
//!
//! The reference code below is the original linear-scan semantics; the indexed
//! lookups must agree with it on every id, including duplicates.

use super::tests::consume;
use super::*;

const FIXTURE: &str = include_str!(
    "../../../../../../fixtures/perl_lsp_facts_exporter/expected/ripr-perl-source-test-oracle-facts-v1.json"
);

fn base_packet() -> Result<PerlFactPacket, String> {
    consume(FIXTURE)
}

/// Original `verify_command_for_test` semantics, as a position.
fn linear_verify(packet: &PerlFactPacket, test_id: &str) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, c) in packet.verify_commands.iter().enumerate() {
        if c.test_id.as_deref() == Some(test_id) && !c.argv.is_empty() {
            match best {
                Some(b) if packet.verify_commands[b].command_id <= c.command_id => {}
                _ => best = Some(i),
            }
        }
    }
    best
}

fn ptr<T>(v: Option<&T>) -> Option<*const T> {
    v.map(|r| r as *const T)
}

/// Grow the packet with `n` extra changes/tests/relations/commands.
fn scale(packet: &mut PerlFactPacket, n: usize) -> Result<(), String> {
    let change = packet.changes.first().cloned().ok_or("change")?;
    let test = packet.tests.first().cloned().ok_or("test")?;
    let relation = packet.relations.first().cloned().ok_or("relation")?;
    let command = packet.verify_commands.first().cloned().ok_or("command")?;
    for i in 0..n {
        let mut c = change.clone();
        c.change_id = format!("change:scale:{i}");
        let mut t = test.clone();
        t.test_id = format!("test:scale:{i}");
        let mut r = relation.clone();
        r.relation_id = format!("relation:scale:{i}");
        r.change_id = c.change_id.clone();
        r.test_id = t.test_id.clone();
        r.oracle_id = None;
        let mut v = command.clone();
        v.command_id = format!("command:scale:{i}");
        v.test_id = Some(t.test_id.clone());
        packet.changes.push(c);
        packet.tests.push(t);
        packet.relations.push(r);
        packet.verify_commands.push(v);
    }
    Ok(())
}

#[test]
fn indexed_lookups_match_linear_first_match_semantics_with_duplicates() -> Result<(), String> {
    let mut packet = base_packet()?;
    // Duplicate ids (ingestion rejects these, but the lookup contract is
    // first-match) plus verify-command ties and empty-argv exclusions.
    let dup_test = packet.tests.first().cloned().ok_or("test")?;
    let mut later = dup_test.clone();
    later.name = "later duplicate".to_string();
    packet.tests.push(later);
    let dup_file = packet.files.first().cloned().ok_or("file")?;
    let mut later_file = dup_file.clone();
    later_file.path = "lib/Later.pm".to_string();
    packet.files.push(later_file);
    let dup_rel = packet.relations.first().cloned().ok_or("rel")?;
    let mut later_rel = dup_rel.clone();
    later_rel.owner_id = "owner:other".to_string();
    packet.relations.push(later_rel);

    let base_cmd = packet.verify_commands.first().cloned().ok_or("cmd")?;
    let tid = dup_test.test_id.clone();
    let mut extra = Vec::new();
    for (id, argv) in [
        ("command:a-empty", vec![]),
        ("command:z-tie", vec!["z"]),
        ("command:m-tie", vec!["m1"]),
        ("command:m-tie", vec!["m2"]),
    ] {
        let mut c = base_cmd.clone();
        c.command_id = id.to_string();
        c.test_id = Some(tid.clone());
        c.argv = argv.into_iter().map(str::to_string).collect();
        extra.push(c);
    }
    packet.verify_commands.extend(extra);

    let mut ids: Vec<String> = Vec::new();
    ids.extend(packet.files.iter().map(|x| x.file_id.clone()));
    ids.extend(packet.tests.iter().map(|x| x.test_id.clone()));
    ids.extend(packet.relations.iter().map(|x| x.relation_id.clone()));
    ids.extend(packet.owners.iter().map(|x| x.owner_id.clone()));
    ids.extend(packet.changes.iter().map(|x| x.change_id.clone()));
    ids.extend(packet.oracles.iter().map(|x| x.oracle_id.clone()));
    ids.extend(packet.provenance.iter().map(|x| x.provenance_id.clone()));
    ids.extend(packet.verify_commands.iter().map(|x| x.command_id.clone()));
    ids.push("missing".to_string());
    assert!(ids.len() > 8);

    for id in &ids {
        assert_eq!(
            ptr(packet.file(id)),
            ptr(packet.files.iter().find(|x| &x.file_id == id)),
            "file {id}"
        );
        assert_eq!(
            ptr(packet.test(id)),
            ptr(packet.tests.iter().find(|x| &x.test_id == id)),
            "test {id}"
        );
        assert_eq!(
            ptr(packet.relation(id)),
            ptr(packet.relations.iter().find(|x| &x.relation_id == id)),
            "relation {id}"
        );
        assert_eq!(
            ptr(packet.owner(id)),
            ptr(packet.owners.iter().find(|x| &x.owner_id == id)),
            "owner {id}"
        );
        assert_eq!(
            ptr(packet.change(id)),
            ptr(packet.changes.iter().find(|x| &x.change_id == id)),
            "change {id}"
        );
        assert_eq!(
            ptr(packet.oracle(id)),
            ptr(packet.oracles.iter().find(|x| &x.oracle_id == id)),
            "oracle {id}"
        );
        assert_eq!(
            ptr(packet.provenance(id)),
            ptr(packet.provenance.iter().find(|x| &x.provenance_id == id)),
            "provenance {id}"
        );
        assert_eq!(
            ptr(packet.verify_command(id)),
            ptr(packet.verify_commands.iter().find(|x| &x.command_id == id)),
            "verify command {id}"
        );
        assert_eq!(
            ptr(packet.verify_command_for_test(id)),
            ptr(linear_verify(&packet, id).map(|i| &packet.verify_commands[i])),
            "verify for test {id}"
        );
    }

    // The tie case really exercised first-wins on equal minima.
    let chosen = packet.verify_command_for_test(&tid).ok_or("chosen")?;
    assert_eq!(chosen.command_id, "command:m-tie");
    assert_eq!(chosen.argv, vec!["m1".to_string()]);

    // Related evidence per change equals the original filter pipeline.
    let mut compared = 0usize;
    for change in &packet.changes {
        let reference: Vec<_> = packet
            .relations
            .iter()
            .filter(|r| r.change_id == change.change_id)
            .filter(|r| r.owner_id == change.owner_id)
            .filter_map(|r| packet.related_test_evidence(r))
            .collect();
        compared += reference.len();
        let actual = packet.related_test_evidence_for_change(&change.change_id);
        assert_eq!(format!("{actual:?}"), format!("{reference:?}"));
    }
    assert!(compared > 0, "equivalence compared real evidence");
    Ok(())
}

#[test]
fn index_is_invisible_to_clone_and_equality() -> Result<(), String> {
    let packet = base_packet()?;
    let _ = packet.index();
    let mut cloned = packet.clone();
    assert_eq!(packet, cloned);
    // A clone rebuilds lazily, so mutating it before first use is honoured.
    let mut extra = cloned.tests.first().cloned().ok_or("test")?;
    extra.test_id = "test:added-after-clone".to_string();
    cloned.tests.push(extra);
    assert!(cloned.test("test:added-after-clone").is_some());
    assert!(packet.test("test:added-after-clone").is_none());
    Ok(())
}

/// Discriminating scale proof: at the 10,000-entry cap the original linear
/// scans cost ~15 s for a single change in a debug build (hours for every
/// change); the indexed path finishes in well under a second. The generous
/// bound only fails on a quadratic regression, not on a slow runner.
#[test]
fn analysis_of_a_packet_at_the_array_cap_is_not_quadratic() -> Result<(), String> {
    let mut packet = base_packet()?;
    scale(&mut packet, 10_000)?;
    assert!(packet.relations.len() > 10_000);

    let started = std::time::Instant::now();
    let mut evidence_total = 0usize;
    for change in &packet.changes {
        evidence_total += packet
            .related_test_evidence_for_change(&change.change_id)
            .len();
        let _ = packet.classify_change_from_related_tests(&change.change_id);
    }
    let elapsed = started.elapsed();
    assert!(evidence_total >= 10_000, "scaled relations resolved");
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "per-change analysis over {} changes took {elapsed:?}",
        packet.changes.len()
    );
    Ok(())
}
