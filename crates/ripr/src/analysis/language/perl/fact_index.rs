//! Keyed lookup index over a Perl fact packet.
//!
//! The packet arrays are capped at 10,000 entries each. Resolving a reference
//! by scanning an array once per change and per relation made analysis
//! quadratic in packet size. This index is built once per packet (lazily, on
//! first lookup) and maps each id to the position of the entry a linear scan
//! would have returned: the first match, so behaviour stays byte-identical even
//! for the duplicate ids that ingestion rejects but unit fixtures may build.
//! It stores positions only, so it never becomes a second source of truth for
//! the facts themselves.

use super::PerlFactPacket;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Default)]
pub(super) struct PacketIndex {
    /// Array lengths at build time; used to catch mutation after first use.
    pub(super) lens: [usize; 8],
    pub(super) files: HashMap<String, usize>,
    pub(super) owners: HashMap<String, usize>,
    pub(super) changes: HashMap<String, usize>,
    pub(super) tests: HashMap<String, usize>,
    pub(super) oracles: HashMap<String, usize>,
    pub(super) relations: HashMap<String, usize>,
    pub(super) provenance: HashMap<String, usize>,
    pub(super) verify_commands: HashMap<String, usize>,
    /// `change_id` -> relation positions, in packet order.
    pub(super) relations_by_change: HashMap<String, Vec<usize>>,
    /// `test_id` -> position of the verify command with the smallest
    /// `command_id` among those with a non-empty argv (first wins on ties).
    pub(super) verify_by_test: HashMap<String, usize>,
}

fn first_positions<'a, T>(
    items: &'a [T],
    key: impl Fn(&'a T) -> &'a str,
) -> HashMap<String, usize> {
    let mut map = HashMap::with_capacity(items.len());
    for (position, item) in items.iter().enumerate() {
        map.entry(key(item).to_string()).or_insert(position);
    }
    map
}

impl PerlFactPacket {
    pub(super) fn lens(&self) -> [usize; 8] {
        [
            self.files.len(),
            self.owners.len(),
            self.changes.len(),
            self.tests.len(),
            self.oracles.len(),
            self.relations.len(),
            self.provenance.len(),
            self.verify_commands.len(),
        ]
    }

    /// Drop the cached index after mutating a packet that was already used.
    /// Production packets are immutable after ingestion.
    #[cfg(test)]
    pub(super) fn reset_index(&mut self) {
        self.index = LazyPacketIndex::default();
    }
}

impl PacketIndex {
    pub(super) fn build(packet: &PerlFactPacket) -> Self {
        let mut relations_by_change: HashMap<String, Vec<usize>> = HashMap::new();
        for (position, relation) in packet.relations.iter().enumerate() {
            relations_by_change
                .entry(relation.change_id.clone())
                .or_default()
                .push(position);
        }

        let mut verify_by_test: HashMap<String, usize> = HashMap::new();
        for (position, command) in packet.verify_commands.iter().enumerate() {
            let Some(test_id) = command.test_id.as_deref() else {
                continue;
            };
            if command.argv.is_empty() {
                continue;
            }
            match verify_by_test.get(test_id) {
                // Strictly smaller only: `Iterator::min_by` keeps the first of
                // equal minima.
                Some(&best) if packet.verify_commands[best].command_id <= command.command_id => {}
                _ => {
                    verify_by_test.insert(test_id.to_string(), position);
                }
            }
        }

        Self {
            lens: packet.lens(),
            files: first_positions(&packet.files, |f| f.file_id.as_str()),
            owners: first_positions(&packet.owners, |o| o.owner_id.as_str()),
            changes: first_positions(&packet.changes, |c| c.change_id.as_str()),
            tests: first_positions(&packet.tests, |t| t.test_id.as_str()),
            oracles: first_positions(&packet.oracles, |o| o.oracle_id.as_str()),
            relations: first_positions(&packet.relations, |r| r.relation_id.as_str()),
            provenance: first_positions(&packet.provenance, |p| p.provenance_id.as_str()),
            verify_commands: first_positions(&packet.verify_commands, |v| v.command_id.as_str()),
            relations_by_change,
            verify_by_test,
        }
    }
}

/// Lazily built [`PacketIndex`] that is invisible to packet equality, cloning
/// and deserialization (a clone starts empty and rebuilds on demand).
#[derive(Debug, Default)]
pub(super) struct LazyPacketIndex(OnceLock<PacketIndex>);

impl LazyPacketIndex {
    pub(super) fn get_or_build(&self, packet: &PerlFactPacket) -> &PacketIndex {
        self.0.get_or_init(|| PacketIndex::build(packet))
    }
}

impl Clone for LazyPacketIndex {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for LazyPacketIndex {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for LazyPacketIndex {}
