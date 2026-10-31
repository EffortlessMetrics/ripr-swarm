//! Provider-bound generated tests for the first #6643 proptest profile.
//!
//! Raw parser/cache facts stay quarantined. Only this post-parse producer can
//! retain the exact generated test plus its audited assertion interpretation.
//! A copied or changed test fact cannot borrow that receipt.

use super::{RustIndex, TestFact};
use crate::analysis::syntax::{proptest_expansions, ra::summarize_file_with_parser};
use crate::analysis::workspace::{MetadataProbeFailure, verified_proptest_identity_result};
use std::collections::BTreeSet;
use std::sync::Arc;

#[cfg(test)]
mod metadata_failure_tests;

#[derive(Clone, Debug)]
pub(super) struct ProptestReceipt {
    pub(super) test: TestFact,
    pub(super) source: Arc<str>,
    pub(super) line_offset: usize,
    pub(super) name_scopes: Arc<crate::analysis::syntax::NameScopes>,
}

/// A failed attempt is diagnostic context, never a generated-test receipt.
#[derive(Clone, Debug)]
pub(super) struct ProptestMetadataFailure {
    failure: MetadataProbeFailure,
    source: Arc<str>,
}

impl RustIndex {
    pub(crate) fn proptest_metadata_failure(
        &self,
        path: &std::path::Path,
        macro_line: usize,
        macro_name: &str,
    ) -> Option<MetadataProbeFailure> {
        let failed = self.proptest_metadata_failures.get(&(
            path.to_path_buf(),
            macro_line,
            macro_name.to_string(),
        ))?;
        // Even a same-line/same-name replacement cannot borrow an older
        // invocation's operational explanation from an incrementally edited index.
        (self.files.get(path)?.data.source == failed.source).then_some(failed.failure)
    }
}

pub(super) fn register_proptest_tests(
    index: &mut RustIndex,
    root: &std::path::Path,
) -> Result<(), String> {
    index.proptest_receipts.clear();
    index.proptest_metadata_failures.clear();
    let files = index
        .files()
        .iter()
        .filter(|(_, file)| !file.used_lexical_fallback && file.source.contains("proptest"))
        .map(|(path, file)| (path.clone(), proptest_expansions(&file.source)))
        .filter(|(_, expansions)| !expansions.is_empty())
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Ok(());
    }
    let identity = match verified_proptest_identity_result(root) {
        Ok(Some(identity)) => identity,
        Ok(None) => return Ok(()),
        Err(failure) => {
            for (path, expansions) in files {
                crate::analysis::cancellation::checkpoint()?;
                let Some(file) = index.files.get(&path) else {
                    continue;
                };
                let source = Arc::clone(&file.data.source);
                // Only the syntax reader's supported invocations reached this
                // verification. Opaque/unsupported neighboring macros keep
                // their generic limitation and acquire no provider diagnosis.
                for expansion in expansions {
                    index.proptest_metadata_failures.insert(
                        (path.clone(), expansion.macro_line, expansion.macro_name),
                        ProptestMetadataFailure {
                            failure,
                            source: Arc::clone(&source),
                        },
                    );
                }
            }
            return Ok(());
        }
    };
    let mut known = index
        .tests()
        .iter()
        .map(|test| (test.file.clone(), test.start_line, test.name.clone()))
        .collect::<BTreeSet<_>>();
    for (path, expansions) in files {
        crate::analysis::cancellation::checkpoint()?;
        if !identity.admits_proptest_target(root, &path) {
            continue;
        }
        let mut groups = std::collections::BTreeMap::<_, Vec<_>>::new();
        for expansion in expansions {
            groups
                .entry((expansion.macro_line, expansion.macro_name.clone()))
                .or_default()
                .push(expansion);
        }
        for ((macro_line, macro_name), properties) in groups {
            // Admission is atomic per macro. A refused/duplicate function must
            // not lose its limitation because a neighboring property worked.
            let receipts = properties
                .into_iter()
                .map(|expansion| {
                    let facts = summarize_file_with_parser(&path, &expansion.source).ok()?;
                    let mut tests = facts.tests.into_iter();
                    let mut test = tests.next()?;
                    if tests.next().is_some() {
                        return None;
                    }
                    let offset = expansion.function_line.checked_sub(test.start_line)?;
                    test.start_line += offset;
                    test.end_line += offset;
                    for call in &mut test.calls {
                        call.line += offset;
                    }
                    for assertion in &mut test.assertions {
                        assertion.line += offset;
                    }
                    for literal in &mut test.literals {
                        literal.line += offset;
                    }
                    Some(ProptestReceipt {
                        test,
                        source: Arc::from(expansion.source),
                        line_offset: offset,
                        name_scopes: Arc::new(expansion.name_scopes),
                    })
                })
                .collect::<Option<Vec<_>>>();
            let Some(receipts) = receipts else { continue };
            let keys = receipts
                .iter()
                .map(|receipt| {
                    (
                        receipt.test.file.clone(),
                        receipt.test.start_line,
                        receipt.test.name.clone(),
                    )
                })
                .collect::<BTreeSet<_>>();
            if keys.len() != receipts.len() || keys.iter().any(|key| known.contains(key)) {
                continue;
            }
            known.extend(keys);
            for receipt in receipts {
                let key = (
                    receipt.test.file.clone(),
                    receipt.test.start_line,
                    receipt.test.name.clone(),
                );
                let id = index.test_facts.allocate(receipt.test.clone());
                index.test_order.push(id);
                if let Some(file) = index.files.get_mut(&path) {
                    file.tests.push(id);
                }
                index.proptest_receipts.insert(key, receipt);
            }
            if let Some(file) = index.files.get_mut(&path) {
                file.tests.sort_by(|&left, &right| {
                    let left = &index.test_facts[left];
                    let right = &index.test_facts[right];
                    left.start_line
                        .cmp(&right.start_line)
                        .then(left.end_line.cmp(&right.end_line))
                        .then(left.name.cmp(&right.name))
                });
                file.data
                    .unresolved_property_macros
                    .retain(|property| property.line != macro_line || property.name != macro_name);
            }
        }
    }
    Ok(())
}
