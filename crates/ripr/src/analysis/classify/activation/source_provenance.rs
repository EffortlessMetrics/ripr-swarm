//! Source identity for values captured within one indexed test.
//!
//! Derived values without an originating-test witness remain unsourced.
//! This leaf assigns metadata only and makes no admission decision.

use crate::domain::ValueFact;
use std::path::Path;

pub(super) fn attach_test_source(facts: &mut [ValueFact], source_file: &Path) {
    if source_file.as_os_str().is_empty() {
        return;
    }
    for fact in facts {
        fact.source_file = Some(source_file.to_path_buf());
    }
}
