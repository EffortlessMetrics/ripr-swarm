//! Provider-bound generated tests for the first #6643 proptest profile.
//!
//! Raw parser/cache facts stay quarantined. Only this post-parse producer can
//! retain the exact generated test plus its audited assertion interpretation.
//! A copied or changed test fact cannot borrow that receipt.

use super::{RustIndex, TestFact};
use crate::analysis::syntax::{proptest_expansions, ra::summarize_file_with_parser};
use crate::analysis::workspace::verified_proptest_identity;
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct ProptestReceipt {
    pub(super) test: TestFact,
    pub(super) source: Arc<str>,
    pub(super) line_offset: usize,
    pub(super) name_scopes: Arc<crate::analysis::syntax::NameScopes>,
    pub(super) singleton_value: Option<(String, String)>,
}

pub(super) fn register_proptest_tests(
    index: &mut RustIndex,
    root: &std::path::Path,
) -> Result<(), String> {
    index.proptest_receipts.clear();
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
    let Some(identity) = verified_proptest_identity(root) else {
        return Ok(());
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
        let standard_crates_unshadowed = !expansions
            .iter()
            .any(|expansion| expansion.singleton_value.is_some())
            || identity.proptest_standard_crates_unshadowed(root, &path);
        let mut groups = std::collections::BTreeMap::<_, Vec<_>>::new();
        for expansion in expansions {
            groups
                .entry((expansion.macro_line, expansion.macro_name.clone()))
                .or_default()
                .push(expansion);
        }
        for ((macro_line, macro_name), properties) in groups {
            // Independently admitted siblings keep receipts, but a partially
            // understood macro must retain its unresolved-property limitation.
            let complete = properties.iter().all(|property| {
                property.macro_complete
                    && (property.singleton_value.is_none() || standard_crates_unshadowed)
            });
            let receipts = properties
                .into_iter()
                .filter(|property| property.singleton_value.is_none() || standard_crates_unshadowed)
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
                        singleton_value: expansion.singleton_value,
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
                if complete {
                    file.data.unresolved_property_macros.retain(|property| {
                        property.line != macro_line || property.name != macro_name
                    });
                }
            }
        }
    }
    Ok(())
}
