mod build;
mod includes;
mod model;

pub use build::build_index;
pub(crate) use build::build_index_from_loaded_files_with_cache;
// Keep compilation-unit rebasing available at the facts facade for index consumers.
pub(crate) use includes::compilation_unit_path_from_parents;
pub use model::{
    CallFact, FileFacts, FunctionFact, FunctionSummary, LiteralFact, OracleFact, ProbeShapeFact,
    ReturnFact, RustIncludeLimitation, RustIndex, TestFact, TestSummary,
};
#[cfg(test)]
pub(crate) use model::{WorkspaceFileAuthority, WorkspaceRootAuthority};
