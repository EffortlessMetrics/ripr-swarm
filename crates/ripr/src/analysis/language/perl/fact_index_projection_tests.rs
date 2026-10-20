//! Full finding-projection controls for the keyed packet index (#8878).
//!
//! Packets here are ingestion-valid at exactly 10,000 entries per capped array
//! and run the whole `packet_to_findings_with_currentness` projection, in two
//! relation shapes. The `*_DIGEST` constants are SHA-256 digests of the full
//! `Debug` rendering of the produced findings, recorded by running this same
//! test body against the pre-index implementation (`9c9c785a5`), so equality is
//! byte-level equivalence with the old linear scans rather than a self-check.
//!
//! This file deliberately uses only items that exist before and after the
//! index, so the identical body can be re-run against either implementation.

use super::tests::{consume, packet_test_options};
use super::*;

const FIXTURE: &str = include_str!(
    "../../../../../../fixtures/perl_lsp_facts_exporter/expected/ripr-perl-source-test-oracle-facts-v1.json"
);

const CAP: usize = 10_000;

const DISTRIBUTED_DIGEST: &str = "c6db3ddd678b1ab48f80c27af44de26202c07130133c2e51a684e31ae5ceb5dc";
const STAR_DIGEST: &str = "55bf95ebed88770bac4a99926203b1ab8da9ef98c9cc76631fec20ea43d63f37";
const DUPLICATES_DIGEST: &str = "f2b6c33177bbca62cf6f52b67fc0e392a037376512995d2a36857952b1b8d9bb";

fn digest(packet: &PerlFactPacket) -> (String, usize) {
    let findings = packet_to_findings_with_currentness(packet, &BTreeMap::new(), |_, _| {
        crate::domain::SourceCurrentness::UnresolvedSubject
    });
    let rendered = format!("{findings:#?}");
    let mut hasher = Sha256::new();
    hasher.update(rendered.as_bytes());
    (hex_bytes(&hasher.finalize()), findings.len())
}

#[derive(Clone, Copy)]
enum Shape {
    /// Change `i` relates to test `i`.
    Distributed,
    /// One change relates to every test.
    Star,
}

/// An ingestion-valid packet with exactly `CAP` entries in every capped array.
fn capped_packet(shape: Shape) -> Result<PerlFactPacket, String> {
    let mut packet = consume(FIXTURE)?;
    let file = packet.files.first().cloned().ok_or("file")?;
    let owner = packet.owners.first().cloned().ok_or("owner")?;
    let change = packet.changes.first().cloned().ok_or("change")?;
    let test = packet.tests.first().cloned().ok_or("test")?;
    let oracle = packet.oracles.first().cloned().ok_or("oracle")?;
    let relation = packet.relations.first().cloned().ok_or("relation")?;
    let command = packet.verify_commands.first().cloned().ok_or("command")?;

    packet.files.clear();
    packet.owners.clear();
    packet.changes.clear();
    packet.tests.clear();
    packet.oracles.clear();
    packet.relations.clear();
    packet.verify_commands.clear();
    packet.limitations.clear();
    packet.dynamic_boundaries.clear();

    for i in 0..CAP {
        let mut f = file.clone();
        f.file_id = format!("file:f-{i}");
        f.path = format!("lib/My/App-{i}.pm");
        packet.files.push(f);

        let mut o = owner.clone();
        o.owner_id = format!("perl:My::App::owner_{i}");
        o.file_id = format!("file:f-{i}");
        packet.owners.push(o);

        let (change_owner, change_id) =
            (format!("perl:My::App::owner_{i}"), format!("change:c-{i}"));
        let mut c = change.clone();
        c.change_id = change_id;
        c.owner_id = change_owner;
        c.file_id = format!("file:f-{i}");
        packet.changes.push(c);

        let mut t = test.clone();
        t.test_id = format!("test:t-{i}");
        t.file_id = format!("file:f-{}", (i + 1) % CAP);
        packet.tests.push(t);

        let mut k = oracle.clone();
        k.oracle_id = format!("oracle:k-{i}");
        k.test_id = format!("test:t-{i}");
        k.target_owner_id = Some(match shape {
            Shape::Distributed => format!("perl:My::App::owner_{i}"),
            Shape::Star => "perl:My::App::owner_0".to_string(),
        });
        packet.oracles.push(k);

        let mut r = relation.clone();
        r.relation_id = format!("relation:r-{i}");
        r.test_id = format!("test:t-{i}");
        r.oracle_id = Some(format!("oracle:k-{i}"));
        match shape {
            Shape::Distributed => {
                r.change_id = format!("change:c-{i}");
                r.owner_id = format!("perl:My::App::owner_{i}");
            }
            Shape::Star => {
                r.change_id = "change:c-0".to_string();
                r.owner_id = "perl:My::App::owner_0".to_string();
            }
        }
        packet.relations.push(r);

        let mut v = command.clone();
        v.command_id = format!("verify:v-{i}");
        v.test_id = Some(format!("test:t-{i}"));
        packet.verify_commands.push(v);
    }
    packet.packet_fingerprint = packet.recompute_packet_fingerprint();
    packet.validate_ingestion(&packet_test_options())?;
    for len in [
        packet.files.len(),
        packet.owners.len(),
        packet.changes.len(),
        packet.tests.len(),
        packet.oracles.len(),
        packet.relations.len(),
    ] {
        assert_eq!(len, CAP, "every capped array sits exactly at the cap");
    }
    Ok(packet)
}

/// Mixed duplicate keys and command/provenance edge cases (not ingestion-valid
/// by design: it pins first-match behaviour that ingestion makes unreachable).
fn duplicates_packet() -> Result<PerlFactPacket, String> {
    let mut packet = consume(FIXTURE)?;
    let test = packet.tests.first().cloned().ok_or("test")?;
    let mut later_test = test.clone();
    later_test.name = "later duplicate".to_string();
    later_test.file_id = "file:missing".to_string();
    packet.tests.push(later_test);
    let file = packet.files.first().cloned().ok_or("file")?;
    let mut later_file = file.clone();
    later_file.path = "lib/Later.pm".to_string();
    later_file.provenance_refs = vec!["provenance:later".to_string()];
    packet.files.push(later_file);
    let relation = packet.relations.first().cloned().ok_or("relation")?;
    let mut later_relation = relation.clone();
    later_relation.test_id = "test:missing".to_string();
    later_relation.oracle_id = None;
    packet.relations.push(later_relation);
    let oracle = packet.oracles.first().cloned().ok_or("oracle")?;
    let mut later_oracle = oracle.clone();
    later_oracle.test_id = "test:other".to_string();
    packet.oracles.push(later_oracle);
    let provenance = packet.provenance.first().cloned().ok_or("provenance")?;
    packet.provenance.push(provenance);

    let command = packet.verify_commands.first().cloned().ok_or("command")?;
    let test_id = test.test_id.clone();
    for (id, argv) in [
        ("verify:a-empty", vec![]),
        ("verify:z-tie", vec!["z"]),
        ("verify:m-tie", vec!["m1"]),
        ("verify:m-tie", vec!["m2"]),
    ] {
        let mut c = command.clone();
        c.command_id = id.to_string();
        c.test_id = Some(test_id.clone());
        c.argv = argv.into_iter().map(str::to_string).collect();
        packet.verify_commands.push(c);
    }
    Ok(packet)
}

fn check(packet: &PerlFactPacket, expected: &str, label: &str) {
    let started = std::time::Instant::now();
    let (actual, findings) = digest(packet);
    let elapsed = started.elapsed();
    assert!(findings > 0, "{label}: projection produced findings");
    assert_eq!(actual, expected, "{label}: findings digest");
    if label != "duplicates" {
        // The pre-index code took 41 s (distributed) and 28 s (star) on this
        // input in a debug build under load; the indexed path takes ~1-2 s.
        // The bound sits between them, so the old quadratic scans fail it.
        assert!(
            elapsed < std::time::Duration::from_secs(12),
            "{label}: full projection at the cap took {elapsed:?}"
        );
    }
}

#[test]
fn distributed_cap_projection_matches_pre_index_output() -> Result<(), String> {
    let packet = capped_packet(Shape::Distributed)?;
    check(&packet, DISTRIBUTED_DIGEST, "distributed");
    Ok(())
}

#[test]
fn star_cap_projection_matches_pre_index_output() -> Result<(), String> {
    let packet = capped_packet(Shape::Star)?;
    check(&packet, STAR_DIGEST, "star");
    Ok(())
}

#[test]
fn duplicate_key_projection_matches_pre_index_output() -> Result<(), String> {
    check(&duplicates_packet()?, DUPLICATES_DIGEST, "duplicates");
    Ok(())
}
