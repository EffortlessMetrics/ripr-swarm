//! One shared record per distinct related-test grip (#5341, #5362).
//!
//! Seams relate to the same tests over and over: on ripr-swarm 10,000 seams
//! hold 1.28M related-test entries but only about 15k distinct records.
//! Records are immutable, so sharing one does not couple seams. Evidence
//! production and the sharded cache loader both intern through this type.

use super::RelatedTestGrip;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Default)]
pub(crate) struct SharedGrips {
    records: HashSet<Arc<RelatedTestGrip>>,
}

impl SharedGrips {
    /// The shared copy of `grip`: an equal record already handed out, or
    /// `grip` itself, now shared.
    pub(crate) fn share(&mut self, grip: RelatedTestGrip) -> Arc<RelatedTestGrip> {
        match self.records.get(&grip) {
            Some(existing) if same_path_spelling(existing, &grip) => Arc::clone(existing),
            Some(_) => Arc::new(grip),
            None => {
                let grip = Arc::new(grip);
                self.records.insert(Arc::clone(&grip));
                grip
            }
        }
    }

    /// Like [`Self::share`] for a record that is already shared elsewhere,
    /// such as one decoded from a cache shard.
    pub(crate) fn share_arc(&mut self, grip: &Arc<RelatedTestGrip>) -> Arc<RelatedTestGrip> {
        match self.records.get(grip.as_ref()) {
            Some(existing) if same_path_spelling(existing, grip) => Arc::clone(existing),
            Some(_) => Arc::clone(grip),
            None => {
                self.records.insert(Arc::clone(grip));
                Arc::clone(grip)
            }
        }
    }

    /// Forget records no seam holds any more, so a streamed review does not
    /// keep the tests of windows its consumer already discarded. Records a
    /// retained seam still holds stay shared.
    pub(crate) fn release_unheld(&mut self) {
        self.records.retain(|grip| Arc::strong_count(grip) > 1);
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
