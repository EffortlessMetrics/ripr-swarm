# Parse-workload attribution on exact a5

This is source inspection, not an instrumented parse counter. The submitted
source is not modified. Timing alone cannot assign a regression exclusively to
one parse site.

* `analysis/syntax/nesting.rs:43` parses a clean complete source with
  ra_ap_syntax after a lexical nesting-budget scan. Its `parse_source_file`
  first tries current edition and retries 2021 only on parse errors. The
  generated workloads use ordinary valid edition-2024 syntax, so this retry
  is not expected. Successful native compilation and absence of lexical
  fallback in the analyzer are separate setup/output checks.
* `classify/owner_pin.rs:3414`'s `function_declaration_is_async` reads the
  complete file from `RustIndex`, calls `parse_clean_source_file`, walks
  function descendants and matches name plus declaration line. The line
  query scans the source prefix for newlines. It has no declaration-answer
  memo. `declares_async` delegates to it, preserving the old owner-query
  default while the new helper traversal requires `Some(false)`.
* `test_grip_evidence/related_tests/context.rs:3993`'s bounded recursive
  helper traversal first finds same-file helper candidates from caller call
  names. Before lexical entry identity resolution, each eligible helper is
  checked with the complete-file async query (line 4020). Same names in other
  inline modules therefore can incur the async parse before identity refusal.
  An async entry is refused before body traversal. Sync chains visit retained
  entry, intermediate and last-hop helpers, then receiver syntax.
* `context.rs:614` memoizes only `(test_index, owner_id) -> bool` helper reach,
  not parsed files or declarations. Stored hits avoid repeating that final
  query; computation occurs between the locked lookup and locked insertion,
  so parallel seams can also duplicate an in-flight computation for the same
  key. That concurrency effect is a source-based possibility, not a measured
  count. The memo does not remove work for distinct tests/owners. Test
  and owner counts both grow in the generated scaling family. Shared common
  accessor/helper names intentionally stress this candidate fanout.
* `classify/related_tests.rs:3120`'s `executable_call_resolves` separately
  parses the caller body, walks executed descendants, and can parse assertion
  operands. `free_helper_call_resolves` also reaches same-file lexical identity,
  whose name-scope lookup (`call_identity.rs:607`, `syntax/name_scope.rs:111`)
  parses complete file syntax. `OwnerPinSyntax` caches exist but new helper
  calls construct local syntax owners; there is no global parsed-file cache
  established by this code. These additional paths mean a parse-cost claim
  must not attribute all overhead solely to the async query.
* `test_grip_evidence.rs:257` shares `CompactGripContext` across the full
  evidence pass as well. Full and compact analysis both can reach this work;
  they differ in evidence payload and other analysis. `output/render.rs:185`
  reaches compact via `repo-exposure-summary-json`. `repo-badge-json` uses
  full analysis for canonical actionability (line 529), so it is not the
  compact benchmark entry point.
* Persistent classified-cache hits bypass these cold compute paths.
  Fresh-cache and hit-cache measurements are reported separately. A warm
  improvement cannot clear a cold-parse regression.

At fixed 64 modules, split-file versus single-file layouts retain the same
local helper bodies, names and tests but change the containing file bytes and
same-file candidate population. This control can support a file/fanout cost
hypothesis but cannot separate those two factors by itself. Free-function
direct-call controls avoid the new receiver/helper path. Retained fixture
cases cover mixed sync/async, method helpers, dormant blocks and unsupported
polling shapes; output changes there are expected semantic changes, not
timing-only wins. Exact runtime parse counts and profiles are not claimed.
