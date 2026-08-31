mod cargo_targets;
mod classify;
mod discover;
mod path_dependencies;
mod select;
mod source_role;

pub(crate) use cargo_targets::{context_for_files, declared_crate_root_paths_from_manifest};
pub(crate) use path_dependencies::PathDependencyAdjacency;
pub(crate) use source_role::{SourceRoleContext, classify_with};

pub(crate) use classify::package_root;
pub(crate) use discover::discover_preview_language_files;
pub use discover::discover_rust_files;
pub use select::select_rust_files_for_mode;
