//! Provider-bound Bolero runner-to-property receipts (#10758).

use super::{RustIndex, TestFact};
use crate::analysis::syntax::bolero_profiles;
use crate::analysis::workspace::verified_bolero_identity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BoleroReceipt {
    pub(super) test: TestFact,
    pub(super) input_type: String,
    pub(super) complete: bool,
}

pub(super) fn register_bolero_tests(
    index: &mut RustIndex,
    root: &std::path::Path,
) -> Result<(), String> {
    index.bolero_receipts.clear();
    let Some(identity) = verified_bolero_identity(root) else {
        return Ok(());
    };
    let files = index
        .files()
        .iter()
        .filter(|(_, file)| !file.used_lexical_fallback && file.source.contains("bolero"))
        .map(|(path, file)| (path.clone(), bolero_profiles(&file.source)))
        .filter(|(_, profiles)| !profiles.is_empty())
        .collect::<Vec<_>>();
    for (path, profiles) in files {
        crate::analysis::cancellation::checkpoint()?;
        if !identity.admits_bolero_target(root, &path) {
            continue;
        }
        for profile in profiles {
            if !profile.complete {
                continue;
            }
            let Some(test) = index.tests().iter().find(|test| {
                test.file == path
                    && test.start_line == profile.function_line
                    && test.name == profile.function_name
            }) else {
                continue;
            };
            index.bolero_receipts.insert(
                (path.clone(), test.start_line, test.name.clone()),
                BoleroReceipt {
                    test: test.clone(),
                    input_type: profile.input_type,
                    complete: profile.complete,
                },
            );
        }
    }
    Ok(())
}
