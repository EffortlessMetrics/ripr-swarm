//! Workspace facade-crate reach limitation for Rust `no_static_path` findings
//! (#9982).
//!
//! A facade crate re-exports an owner crate (`pub use inner::*;`) and carries
//! the tests, so a test calls the owner's code through `facade::Ext` and never
//! spells the owner or its crate. The related-test search keeps tests of
//! another package out unless they call the owner's name, so such a finding
//! reads as a definite `no_static_path` ("No static test path found") while a
//! candidate test path exists that ripr does not trace.
//!
//! ## Fail-closed design
//!
//! - The limitation is only a named non-claim; the classification stays
//!   `no_static_path` and the test is never added to `related_tests`, so
//!   nothing is promoted toward `exposed` and no repair signal is created.
//!   Naming the limit withholds the repair gap, the safe direction.
//! - The evidence is structural, not lexical: a complete path-dependency graph
//!   with a forward `[dependencies]` edge from the facade package to the
//!   owner's package, a `pub use <owner crate>` in the facade's `src/lib.rs`,
//!   and an integration test of the facade package that names the facade
//!   crate in a path (`facade::...`).
//! - A missing edge, an unrelated package, a private `use`, or a test that
//!   never names the facade leaves the finding exactly as it was.
//! - Re-exports are not resolved. No item is matched to the owner.
//! - The facade crate's name is read as its manifest directory name (`-`
//!   folded to `_`), the Cargo default. A facade renamed in its manifest
//!   keeps the previous behavior.

use super::related_tests::{
    DependencyEdgeContext, nearest_manifest_identity, normalize_path, strip_comments_and_strings,
};
use crate::analysis::facts::{FunctionSummary, RustIndex};
use crate::analysis::seam_cache::PathDependencySection;
use crate::analysis::workspace::PathDependencyGraphStatus;
use std::path::PathBuf;

/// The facade-crate test that may reach the owner through a re-export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct FacadeReachWitness {
    pub test_name: String,
    pub test_file: PathBuf,
    pub test_line: usize,
    /// The facade crate's name as the test spells it (`facade`).
    pub facade_crate: String,
    /// The owner's crate as the facade declares and re-exports it (`inner`).
    pub owner_crate: String,
    /// Other facade-package integration tests that name the facade.
    pub other_test_count: usize,
}

/// The message emitted as a stop-reason when this limitation fires. A named
/// limitation, not a coverage claim.
pub(in crate::analysis) const RUST_FACADE_REACH_MESSAGE: &str = "ripr saw a workspace crate that re-exports the crate holding this change, with an integration test that uses it \
     through a re-export path it does not resolve. \
     This is not a coverage assessment -- ripr cannot confirm or deny \
     that the change is observed.";

/// The best facade-crate test for `owner`, or `None` when the workspace
/// graph, the re-export or the test's use of the facade is not established.
pub(in crate::analysis) fn facade_reach_witness(
    index: &RustIndex,
    context: &DependencyEdgeContext<'_>,
    owner: &FunctionSummary,
) -> Option<FacadeReachWitness> {
    if context.adjacency.status() != PathDependencyGraphStatus::Complete {
        return None;
    }
    let owner_manifest = nearest_manifest_identity(context.manifest_dir_prefixes, &owner.file)?;
    let dependents = context.adjacency.reverse_neighbors(&owner_manifest)?;
    let mut best: Option<FacadeReachWitness> = None;
    let mut count = 0usize;
    for dependent in dependents {
        let Some(directory) = dependent.strip_suffix("Cargo.toml") else {
            continue;
        };
        let Some(facade_crate) = crate_name_of_directory(directory) else {
            continue;
        };
        let Some(declarations) = context
            .adjacency
            .forward_dependency_declarations(dependent, &owner_manifest)
        else {
            continue;
        };
        // A re-export needs a normal dependency; a dev-dependency is not
        // visible to the library.
        let owner_crates: Vec<String> = declarations
            .iter()
            .filter(|(section, _)| *section == PathDependencySection::Dependencies)
            .map(|(_, name)| name.replace('-', "_"))
            .collect();
        let Some(owner_crate) = owner_crates
            .into_iter()
            .find(|name| lib_reexports_crate(index, directory, name))
        else {
            continue;
        };
        for test in index.tests().iter() {
            if !crate::analysis::rust_index::is_test_file(&test.file)
                || nearest_manifest_identity(context.manifest_dir_prefixes, &test.file).as_deref()
                    != Some(dependent.as_str())
                || !file_names_path_root(index, &test.file, &facade_crate)
            {
                continue;
            }
            count += 1;
            let candidate = FacadeReachWitness {
                test_name: test.name.clone(),
                test_file: test.file.clone(),
                test_line: test.start_line,
                facade_crate: facade_crate.clone(),
                owner_crate: owner_crate.clone(),
                other_test_count: 0,
            };
            let key =
                |w: &FacadeReachWitness| (w.test_file.clone(), w.test_line, w.test_name.clone());
            if best
                .as_ref()
                .is_none_or(|current| key(&candidate) < key(current))
            {
                best = Some(candidate);
            }
        }
    }
    let mut witness = best?;
    witness.other_test_count = count.saturating_sub(1);
    Some(witness)
}

/// The crate identifier of a package whose manifest sits in `directory`
/// (`crates/my-facade/` is `my_facade`). `None` for the workspace root
/// directory, whose name is not the package's.
fn crate_name_of_directory(directory: &str) -> Option<String> {
    let name = directory.trim_end_matches('/').rsplit('/').next()?;
    if name.is_empty() || name == "." {
        return None;
    }
    let name = name.replace('-', "_");
    name.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        .then_some(name)
}

/// Whether `<directory>src/lib.rs` re-exports `crate_name` with `pub use`.
fn lib_reexports_crate(index: &RustIndex, directory: &str, crate_name: &str) -> bool {
    let lib = format!("{directory}src/lib.rs");
    index
        .files()
        .iter()
        .filter(|(path, _)| normalize_path(path) == lib)
        .any(|(_, facts)| {
            let stripped = strip_comments_and_strings(&facts.data().source);
            has_pub_use_of(&stripped, crate_name)
        })
}

/// `pub use <name>` followed by a path separator, `;`, `as` or a brace
/// boundary, as in `pub use inner::*;`, `pub use inner as other;` or
/// `pub use ::inner::Item;`. `pub(crate) use` is not a re-export.
fn has_pub_use_of(stripped: &str, name: &str) -> bool {
    stripped.match_indices("pub use").any(|(at, keyword)| {
        let before_ok = at == 0
            || !stripped
                .as_bytes()
                .get(at - 1)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
        let rest = stripped[at + keyword.len()..]
            .trim_start()
            .trim_start_matches("::");
        before_ok
            && rest.strip_prefix(name).is_some_and(|after| {
                !after
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
    })
}

/// Whether the file spells `root::` as a path root (`use facade::Ext;`,
/// `facade::io::copy(..)`), outside comments and strings.
fn file_names_path_root(index: &RustIndex, file: &std::path::Path, root: &str) -> bool {
    let Some(facts) = index.files().get(file) else {
        return false;
    };
    let stripped = strip_comments_and_strings(&facts.data().source);
    stripped.match_indices(root).any(|(at, _)| {
        let end = at + root.len();
        let before_ok = at == 0
            || !stripped
                .as_bytes()
                .get(at - 1)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b':');
        before_ok && stripped[end..].starts_with("::")
    })
}

pub(in crate::analysis) fn facade_reach_witness_pointer(witness: &FacadeReachWitness) -> String {
    let others = match witness.other_test_count {
        0 => String::new(),
        1 => " (and 1 other test)".to_string(),
        n => format!(" (and {n} other tests)"),
    };
    format!(
        "{}`{}` ({}:{}) uses workspace crate `{}`, which re-exports `{}`, an entry point that may lead here{}. \
         Inspect it to judge whether this change is observed.",
        crate::domain::TRANSITIVE_REACH_WITNESS_PREFIX,
        witness.test_name,
        witness.test_file.display().to_string().replace('\\', "/"),
        witness.test_line,
        witness.facade_crate,
        witness.owner_crate,
        others
    )
}

pub(in crate::analysis) fn facade_reach_limitation_detail_lines(
    witness: &FacadeReachWitness,
    owner_name: &str,
) -> [String; 4] {
    [
        format!(
            "{}test `{}` ({}:{}) -> crate `{}`",
            crate::domain::LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX,
            witness.test_name,
            witness.test_file.display().to_string().replace('\\', "/"),
            witness.test_line,
            witness.facade_crate
        ),
        format!(
            "{}crate `{}` re-export of `{}` -> owner `{}` through a re-export path ripr does not resolve",
            crate::domain::LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX,
            witness.facade_crate,
            witness.owner_crate,
            owner_name
        ),
        format!(
            "{}analysis/rust-public-api-transitive-reach",
            crate::domain::LIMITATION_ANALYZER_ROUTE_PREFIX
        ),
        format!(
            "{}named limitation only; ripr cannot confirm or deny that this path observes the change",
            crate::domain::LIMITATION_NON_CLAIM_PREFIX
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pub_use_of_matches_reexports_only() {
        assert!(has_pub_use_of("pub use inner::*;", "inner"));
        assert!(has_pub_use_of("pub use inner as other;", "inner"));
        assert!(has_pub_use_of("pub use ::inner::Item;", "inner"));
        assert!(has_pub_use_of("mod x {}\npub use inner;\n", "inner"));
        assert!(!has_pub_use_of("pub(crate) use inner::*;", "inner"));
        assert!(!has_pub_use_of("use inner::*;", "inner"));
        assert!(!has_pub_use_of("pub use inner_extra::*;", "inner"));
        assert!(!has_pub_use_of("pub use other::inner::*;", "inner"));
    }

    #[test]
    fn crate_name_comes_from_the_manifest_directory() {
        assert_eq!(
            crate_name_of_directory("crates/my-facade/").as_deref(),
            Some("my_facade")
        );
        assert_eq!(
            crate_name_of_directory("facade/").as_deref(),
            Some("facade")
        );
        assert_eq!(crate_name_of_directory(""), None);
    }
}
