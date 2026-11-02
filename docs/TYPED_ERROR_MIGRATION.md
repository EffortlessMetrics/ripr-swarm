# Typed public error migration (#1914 / #1986)

`ripr` is migrating from `Result<_, String>` toward a single public
application error family. The foundation slice lands in #1986:

- [`RiprError`](../crates/ripr/src/ripr_error.rs) and
  [`RiprErrorCategory`](../crates/ripr/src/ripr_error.rs) are
  `#[non_exhaustive]` and exported from the crate root.
- [`try_check_workspace`](../crates/ripr/src/app/check.rs) /
  [`try_check_workspace_with_config`](../crates/ripr/src/app/check.rs)
  project the existing [`CoreError`](../crates/ripr/src/core_error.rs)
  check path onto that boundary.
- Legacy [`check_workspace`] helpers keep returning `String` diagnostics
  with unchanged CLI and human output.

## Conversion rules

1. **Internal first.** Module-local failures stay on private enums or
   `CoreError` until a vertical is ready to expose them.
2. **Project, do not reword.** Public errors carry the same human
   diagnostic as `CoreError` `Display`; machine consumers match
   [`RiprError::category`] and [`RiprError::code`], never message text.
3. **Categories vs codes.** Categories follow the #1986 vocabulary
   (`configuration`, `repository_or_git`, …). Codes reuse check JSON
   refusal identities (`base_unresolvable`, `git_invocation_timeout`, …)
   where they already exist (#6834 / RIPR-SPEC-0241).
4. **Limitations are not internal failures.** Cancellation,
   git timeouts, and named static limitations must not map to
   `internal_invariant`.
5. **Config redaction.** Configuration failures redact file excerpts in
   `Debug` and JSON; public `Display` stays unchanged for CLI parity.

## Next module families (not in the #1986 slice)

| Family | Internal owner today | Public follow-up |
| --- | --- | --- |
| Git / repository helpers outside check | `git`, `domain::GitCandidateSubjectError` | Reuse `repository_or_git` codes; align with candidate subject errors |
| LSP refresh / diagnostics | `lsp`, `CoreError` projections | Map to existing component-outcome kinds (#2811) |
| Agent / repair commands | `cli`, `app::task_first` | #11584 / #11597 machine refusal codes |
| Scope guards / oversized diff | string-matched guards today | #4861 typed identities |
| xtask / repo automation | `Result<_, String>` | Out of published `ripr` API; stay private |

[`check_workspace`]: ../crates/ripr/src/app/check.rs
[`RiprError::category`]: ../crates/ripr/src/ripr_error.rs
[`RiprError::code`]: ../crates/ripr/src/ripr_error.rs
