mod build;
mod includes;
mod model;
mod test_styles;

use std::path::{Path, PathBuf};

pub fn build_index(root: &Path, files: &[PathBuf]) -> Result<model::RustIndex, String> {
    let mut index = build::build_index(root, files)?;
    test_styles::normalize_index_test_styles(&mut index);
    Ok(index)
}

pub(crate) fn build_index_from_loaded_files_with_cache(
    root: &Path,
    files: &[(PathBuf, Vec<u8>)],
) -> Result<build::CachedRustIndex, String> {
    let mut cached = build::build_index_from_loaded_files_with_cache(root, files)?;
    test_styles::normalize_index_test_styles(&mut cached.index);
    Ok(cached)
}

// Keep compilation-unit rebasing available at the facts facade for index consumers.
pub(crate) use includes::compilation_unit_path_from_parents;
pub use model::{
    CallFact, FileFacts, FunctionFact, FunctionSummary, LiteralFact, OracleFact, ProbeShapeFact,
    ReturnFact, RustIncludeLimitation, RustIndex, TestFact, TestSummary,
};
#[cfg(test)]
pub(crate) use model::{WorkspaceFileAuthority, WorkspaceRootAuthority};
