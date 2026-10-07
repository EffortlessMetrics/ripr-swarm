//! One shared record per distinct related-test grip (#5341, #5362).
//!
//! Seams relate to the same tests over and over: on ripr-swarm 10,000 seams
//! hold 1.28M related-test entries but only about 15k distinct records.
//! Records are immutable, so sharing one does not couple seams. Evidence
//! production and the sharded cache loader both intern through this type.

use super::RelatedTestGrip;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Default)]
pub(crate) struct SharedGrips {
    /// Keyed by the first spelling seen; the value holds records that are
    /// equal but spelled differently (`tests/./a.rs`), normally none.
    records: HashMap<Arc<RelatedTestGrip>, Vec<Arc<RelatedTestGrip>>>,
}

impl SharedGrips {
    /// The shared copy of `grip`: an equal record already handed out, or
    /// `grip` itself, now shared.
    pub(crate) fn share(&mut self, grip: RelatedTestGrip) -> Arc<RelatedTestGrip> {
        if let Some(existing) = self.find(&grip) {
            return existing;
        }
        let grip = Arc::new(grip);
        self.remember(&grip);
        grip
    }

    /// Like [`Self::share`] for a record that is already shared elsewhere,
    /// such as one decoded from a cache shard.
    pub(crate) fn share_arc(&mut self, grip: &Arc<RelatedTestGrip>) -> Arc<RelatedTestGrip> {
        if let Some(existing) = self.find(grip) {
            return existing;
        }
        self.remember(grip);
        Arc::clone(grip)
    }

    /// Forget records no seam holds any more, so a streamed review does not
    /// keep the tests of windows its consumer already discarded. Records a
    /// retained seam still holds stay shared.
    pub(crate) fn release_unheld(&mut self) {
        // A group whose first spelling is unheld but whose alternate is held
        // is rekeyed on that alternate, so the first record is freed too.
        let mut rekeyed = Vec::new();
        self.records.retain(|first, others| {
            others.retain(|grip| Arc::strong_count(grip) > 1);
            if Arc::strong_count(first) > 1 {
                return true;
            }
            if !others.is_empty() {
                rekeyed.push(std::mem::take(others));
            }
            false
        });
        for mut others in rekeyed {
            let first = others.remove(0);
            self.records.insert(first, others);
        }
    }

    fn find(&self, grip: &RelatedTestGrip) -> Option<Arc<RelatedTestGrip>> {
        let (first, others) = self.records.get_key_value(grip)?;
        std::iter::once(first)
            .chain(others)
            .find(|shared| same_path_spelling(shared, grip))
            .cloned()
    }

    fn remember(&mut self, grip: &Arc<RelatedTestGrip>) {
        match self.records.get_mut(grip.as_ref()) {
            Some(others) => others.push(Arc::clone(grip)),
            None => {
                self.records.insert(Arc::clone(grip), Vec::new());
            }
        }
    }
}

/// `PathBuf` equality compares components, so `tests/./a.rs` equals
/// `tests/a.rs`. Share only an identical spelling, so the output keeps each
/// seam's own path bytes.
fn same_path_spelling(left: &RelatedTestGrip, right: &RelatedTestGrip) -> bool {
    left.file.as_os_str() == right.file.as_os_str()
        && match (&left.test_target, &right.test_target) {
            (Some(left), Some(right)) => left.file().as_os_str() == right.file().as_os_str(),
            (None, None) => true,
            _ => false,
        }
}
