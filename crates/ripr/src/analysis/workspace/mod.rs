mod assertion_dependency;
mod cargo_input_identity;
mod cargo_targets;
mod classify;
mod discover;
mod module_graph;
mod named_dependent_scope;
mod path_dependencies;
mod project_facts_digest;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "policy:6423-projection-slice: adapter is not wired to command or LSP consumers yet (#6423)"
    )
)]
mod rust_project_facts;
mod select;
mod source_role;

pub(crate) use assertion_dependency::{
    AnyhowIdentity, JsonRootEligibilityMemo, LiteralJsonIdentity, MetadataProbeFailure,
    config_snapshot_identity, eligible_json_root, eligible_manifest, eligible_proptest_root,
    locked_at_audited_version, verified_anyhow_identity, verified_bolero_identity,
    verified_json_identity, verified_proptest_identity, verified_proptest_identity_result,
};
pub(crate) use cargo_input_identity::cargo_input_identity;
pub(crate) use cargo_targets::{
    CargoHarnessVerdict, ManifestInventory, context_for_files,
    declared_crate_root_paths_from_manifest, explicit_target_roots_are_covered,
};
pub(crate) use module_graph::{
    apply_module_graph_evidence, covered_rust_source_manifest_dirs, discovers_targets,
    standalone_packages_have_no_source_aliases, target_manifest_directories,
};
pub(crate) use path_dependencies::{
    PathDependencyAdjacency, PathDependencyGraphStatus, reverse_dependent_scope_expansion,
};
pub(crate) use source_role::{
    DiffOnlySource, SourceRole, SourceRoleContext, classify_with, diff_only_source,
    is_declared_cargo_test_surface, is_declared_cargo_test_surface_in, is_test_surface,
    is_test_surface_path, may_be_diff_only_source, seeds_diff_probes, seeds_repo_seams,
};

pub(crate) use classify::{normalize_path, package_root};
pub(crate) use discover::UnlinkedPythonTests;
#[cfg(test)]
pub(crate) use discover::discover_probe;
pub use discover::discover_rust_files;
pub(crate) use discover::{
    changed_source_files_absent_from_worktree, discover_preview_language_files,
    discover_python_test_files, discover_readable_rust_files, discover_routed_language_files,
    discover_rust_files_tolerating_unreadable, discover_unanalyzed_source_files,
    has_inspectable_source_file, limitations_for_absent_changed_files,
    limitations_for_unreadable_unchanged_paths, refuse_unreadable_dir_over_changed_path,
    split_unreadable_unchanged_files, supplement_absent_file_limitations,
    unreadable_paths_partial_reason, worktree_contains_regular_source_file,
    worktree_regular_source_file,
};
pub(crate) use discover::{is_ignored_dir_name, path_enters_ignored_dir};
pub(crate) use named_dependent_scope::{
    composes_modules, withheld_source_disables_crate_root_routing,
};
pub(crate) use select::{
    module_parent_candidates, select_rust_files_for_mode_with_dependent_packages,
    with_module_context_files,
};
