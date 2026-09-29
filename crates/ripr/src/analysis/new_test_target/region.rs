//! Exact governed inline test-module region identity (#4783-shaped sidecar).
//!
//! Portable identity excludes checkout spelling. Concrete containment uses
//! byte offsets into the current source digest, not a whole-file allowlist.

use super::NewTestProposalBlocker;
use crate::analysis::syntax::{GovernedCfgTestModule, governed_cfg_test_modules};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Exact governed inline test-module region carried with an InlineUnit proposal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct InlineTestRegionAuthority {
    pub(crate) file: PathBuf,
    pub(crate) module_name: String,
    pub(crate) parent_modules: Vec<String>,
    pub(crate) body_start: usize,
    pub(crate) close_brace_start: usize,
    pub(crate) source_digest: String,
}

/// Validate an external edit against a previously admitted inline region.
/// Production text outside the module body must be unchanged.
pub(crate) fn validate_inline_region_edit(
    before: &str,
    after: &str,
    region: &InlineTestRegionAuthority,
) -> Result<(), NewTestProposalBlocker> {
    if source_digest(before) != region.source_digest {
        return Err(NewTestProposalBlocker::StaleSource);
    }
    let before_modules =
        governed_cfg_test_modules(before).ok_or(NewTestProposalBlocker::LexicalFallback)?;
    let after_modules =
        governed_cfg_test_modules(after).ok_or(NewTestProposalBlocker::LexicalFallback)?;
    let before_module = unique_matching_inline(&before_modules, region)?;
    let after_module = unique_matching_inline(&after_modules, region)?;
    if module_prefix(before, before_module)? != module_prefix(after, after_module)?
        || module_suffix(before, before_module)? != module_suffix(after, after_module)?
    {
        return Err(NewTestProposalBlocker::ProductionEdit);
    }
    Ok(())
}

pub(super) fn source_digest(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

fn unique_matching_inline<'a>(
    modules: &'a [GovernedCfgTestModule],
    region: &InlineTestRegionAuthority,
) -> Result<&'a GovernedCfgTestModule, NewTestProposalBlocker> {
    let matches = modules
        .iter()
        .filter(|module| {
            module.is_inline
                && module.name == region.module_name
                && module.parent_modules == region.parent_modules
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(NewTestProposalBlocker::AmbiguousModule);
    }
    Ok(matches[0])
}

fn module_prefix<'a>(
    source: &'a str,
    module: &GovernedCfgTestModule,
) -> Result<&'a str, NewTestProposalBlocker> {
    let body_start = module
        .body_start
        .ok_or(NewTestProposalBlocker::ProductionEdit)?;
    source
        .get(..body_start)
        .ok_or(NewTestProposalBlocker::ProductionEdit)
}

fn module_suffix<'a>(
    source: &'a str,
    module: &GovernedCfgTestModule,
) -> Result<&'a str, NewTestProposalBlocker> {
    let close_brace_start = module
        .close_brace_start
        .ok_or(NewTestProposalBlocker::ProductionEdit)?;
    source
        .get(close_brace_start..)
        .ok_or(NewTestProposalBlocker::ProductionEdit)
}
