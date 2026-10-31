//! Native identity for packet-backed findings, separate from canonical gaps.
//! Producer facts are the payload authority; a hash does not authenticate them.

use super::{OwnerFact, OwnerKind, PerlFactPacket, hex_bytes, normalize_repo_relative};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

type OwnerKey<'a> = (&'a str, &'a str, &'a str, &'a str);

fn named_owner_key<'a>(packet: &'a PerlFactPacket, owner: &'a OwnerFact) -> Option<OwnerKey<'a>> {
    if owner.kind == OwnerKind::Unknown {
        return None;
    }
    let file = packet.file(&owner.file_id)?;
    let package = owner
        .package
        .as_deref()
        .filter(|value| !value.trim().is_empty())?;
    let name = owner
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())?;
    // Ingestion rejects backslashes, the only spelling normalized by
    // normalize_repo_relative, so this key and the hash use the same path.
    Some((file.path.as_str(), owner.kind.as_str(), package, name))
}

/// Length framing keeps arbitrary packet strings (including NULs) from
/// collapsing adjacent fields. Stable and location-bound identities have
/// separate domains, so fallback bytes can never impersonate a stable tuple.
fn digest(domain: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in std::iter::once(&domain).chain(parts.iter()) {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hex_bytes(&hasher.finalize())
}

pub(super) fn probe_ids(packet: &PerlFactPacket) -> BTreeMap<&str, String> {
    let mut owner_counts = BTreeMap::new();
    for owner in &packet.owners {
        if let Some(key) = named_owner_key(packet, owner) {
            *owner_counts.entry(key).or_insert(0_usize) += 1;
        }
    }

    let mut groups = BTreeMap::<String, Vec<_>>::new();
    for change in &packet.changes {
        let (Some(file), Some(owner)) =
            (packet.file(&change.file_id), packet.owner(&change.owner_id))
        else {
            continue;
        };
        let path = normalize_repo_relative(&file.path);
        let owner_key = named_owner_key(packet, owner)
            .filter(|key| owner_counts.get(key) == Some(&1))
            .filter(|_| owner.file_id == change.file_id);
        let observable = match &change.changed_observable.0 {
            None => ("missing", ""),
            Some(None) => ("null", ""),
            Some(Some(value)) => ("value", value.as_str()),
        };
        let base = if let Some((_, kind, package, name)) = owner_key
            && !change.changed_text_digest.trim().is_empty()
        {
            let hash = digest(
                "perl-finding-v1",
                &[
                    &path,
                    kind,
                    package,
                    name,
                    change.behavior_hint.as_str(),
                    &change.changed_text_digest,
                    observable.0,
                    observable.1,
                ],
            );
            format!("probe:perl:{hash}")
        } else {
            // Missing semantic identity stays explicit and location-bound.
            // Fact tokens are ingestion-validated and distinct within a packet;
            // geometry also distinguishes supported same-line changes.
            let owner_start = owner.range.start_line.to_string();
            let owner_column = owner.range.start_column.to_string();
            let start = change.range.start_line.to_string();
            let column = change.range.start_column.to_string();
            let end = change.range.end_line.to_string();
            let end_column = change.range.end_column.to_string();
            let hash = digest(
                "perl-finding-location-v1",
                &[
                    &path,
                    &owner.owner_id,
                    &owner.file_id,
                    owner.kind.as_str(),
                    if owner.package.is_some() {
                        "value"
                    } else {
                        "missing"
                    },
                    owner.package.as_deref().unwrap_or(""),
                    if owner.name.is_some() {
                        "value"
                    } else {
                        "missing"
                    },
                    owner.name.as_deref().unwrap_or(""),
                    &owner_start,
                    &owner_column,
                    &change.change_id,
                    &start,
                    &column,
                    &end,
                    &end_column,
                    change.behavior_hint.as_str(),
                    &change.changed_text_digest,
                    observable.0,
                    observable.1,
                ],
            );
            format!("probe:perl-location:{hash}")
        };
        groups.entry(base).or_default().push((
            change.range.start_line,
            change.range.start_column,
            change.range.end_line,
            change.range.end_column,
            change.change_id.as_str(),
        ));
    }

    let mut ids = BTreeMap::new();
    for (base, mut changes) in groups {
        // Only indistinguishable payloads share a group. Source-order ordinals
        // survive packet array reordering and uniform shifts of the same set;
        // insertion/removal/reordering of identical copies remains ambiguous.
        changes.sort();
        for (index, (_, _, _, _, change_id)) in changes.into_iter().enumerate() {
            let id = if index == 0 {
                base.clone()
            } else {
                format!("{base}.{}", index + 1)
            };
            ids.insert(change_id, id);
        }
    }
    ids
}
