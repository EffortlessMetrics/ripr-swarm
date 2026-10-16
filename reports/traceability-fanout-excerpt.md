# check-traceability

Status: pass

## Why This Matters

Traceability keeps behavior specs, tests, fixtures, code, outputs, and metrics discoverable for long-context human and agent work.

## Scope of this result

A pass checks the authored `[[behavior]]` entries, spec coverage, fixture spec IDs, and the file paths named by registered references. It does not enumerate Rust tests. It does not require every newly added test to appear in `.ripr/traceability.toml`. A `::symbol` suffix remains advisory and is not proof that the symbol exists, has a test role, ran, or establishes the behavior (see #2345).

## Violations

None detected.

## Advisories (non-blocking)

These references point to existing files but carry a `::symbol` suffix that the structural checker cannot verify yet. They do not fail the gate.
```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_auxiliary_binding_does_not_hide_new_workspace_inputs` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_output_caps_cover_members_and_manifest` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_boundary_matches_windows_exclusion_aliases` is not verified by the structural checker (file `crates/ripr/src/analysis/input_boundary.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_typescript_native_projections_match` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_boundary_observes_worker_reads_and_isolates_roots` is not verified by the structural checker (file `crates/ripr/src/analysis/input_boundary.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_boundary_rejects_only_exclusions_below_selected_root` is not verified by the structural checker (file `crates/ripr/src/analysis/input_boundary.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_refuses_output_parent_identity_drift` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_refuses_input_mutations_at_the_native_analysis_boundary` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_input_identity_keeps_non_utf8_paths_without_panicking` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_python_source_stamp_and_navigation_are_native` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_invalid_configuration_refusals_match_native` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_external_module_reads_refuse_without_changing_standalone` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_limited_json_keeps_native_counts_and_admission_policy` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_enters_diff_pipeline_once` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_detects_diff_config_suppression_source_facts_and_membership_drift` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_partial_member_manifest_and_late_drift_fail_without_completion` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_input_byte_cap_is_enforced_before_allocation` is not verified by the structural checker (file `crates/ripr/src/cli/commands/check_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_matches_native_projections_and_binds_members` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_refusal_documents_and_unsupported_exit_are_native` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_render_failures_never_admit_a_successful_set` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

```text
RIPR-SPEC-0001 `tests` symbol suffix `fanout_rejects_stale_destinations_and_conflicting_interfaces` is not verified by the structural checker (file `crates/ripr/tests/cli_smoke/native_fanout.rs` exists; symbol resolution is a planned cargo-proof provider, see #2345)
```

