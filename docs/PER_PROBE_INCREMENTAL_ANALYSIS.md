# Per-probe incremental analysis

Design note for #9784. It is the first acceptance item of that issue: the set
of facts a probe classification reads, how a stored result is validated against
them, and why an unrecorded read cannot produce a stale result. It builds
nothing. The measurement in [Phases](#phases) decides whether the later phases
are worth building. The first implementation below observes index reads only;
it does not yet close Phase A's dependency coverage.

## Problem

A warm `ripr check` after a commit that edits `crates/ripr` costs 49 to 87 s
(#9784), although the file-fact, module-tree and scoped-seam caches are warm.
Those layers cache what is derived from one file or one corpus. What is not
cached is the classification of each probe, which reads the whole index. The
cost left on that diff is a flat profile (evidence gather, transitive reach,
reveal, owner-pin admission, the workspace macro-binding scan), so no single
fix reaches it. A probe whose inputs did not change should keep its finding.

## What a classification reads

`classify_probe_with_candidate_index_and_origins`
(`analysis/classifier.rs`) is a function of the inputs below and of nothing
else. Anything it consults must arrive through one of these, and that is the
property the rest of this note enforces.

| Input | What it is | Dependency kind |
|---|---|---|
| `probe` | `Probe { location, owner, family, delta, before, after, expression, expected_sinks, required_oracles }` | identity (the cache key) |
| `propagation_origin` | the changed diff line the probe propagated from | identity |
| `workspace_complete` | whether the index holds the whole workspace | scalar |
| `index.files()` | per-file source, returns, literals, probe shapes, module declarations, role provenance, item scopes, macro candidates | `File(path)` for `get`; `Universe(files)` for `iter`/`values` |
| `index.functions()`, `index.tests()` | every function and test fact | `Universe(functions)` / `Universe(tests)` for a scan |
| manifest-derived index fields | `package_names`, `root_library_name`, `macro_owned_crates`, `workspace_crates`, `workspace_member_dirs`, `drop_in_manifests`, `member_crates` | scalar, one digest each |
| module and include structure | `execution_module_parents`, `include_parents`, `include_limitations`, `include_targets`, `non_utf8_sources` | one digest each |
| harness and workspace authority | `harness_subjects`, `harness_limitations`, `workspace_authority`, `doctest_sources` | one digest each |
| files read from disk | not through the index: `member_trait_names` (`classify/reach.rs`) walks the `.rs` files under the workspace member directories, indexed or not, and `fallible_check_receipt` (`classify/owner_pin/fallible_check.rs`) canonicalizes the workspace root and consults disk | `Disk(path)` |
| `dependency_edges` | path-dependency adjacency and manifest-prefix inventory | one digest |
| `candidate_index` memos | file `use` scan, owner-pin syntax and workspace macro-binding scan, implicit expansion, unseen-reach names, test value facts, owner-caller names, diff-touched tests, unmounted-build-script test | memo reads (below) |
| `transitive_reach` | bounded caller walk over the index | memo read |
| analyzer environment | binary version, `CACHE_SCHEMA_VERSION`, cfg and feature selection, config, test-intent and suppressions | key |

By count in non-test code under `classify/`, `classifier*`, `probes/` and
`test_grip_evidence*` (grep, indicative): 68 keyed `files().get` against 65
`files().iter()/values()` scans, and 50 `functions()` scans. The scans are the
cost of this design: a read that scans a whole fact set depends on the whole
set.

One output is not a return value. `value_origins` is a shared
`Mutex<ValueOrigins>` that classification appends to. A stored result must
store what the probe appended and replay it on a hit, or a warm run loses
those origins and differs from a cold one.

## Dependency kinds

A read is one of:

- `File(path)`: the content hash of one indexed file, or the distinguished
  value `Absent` when the keyed `files().get(path)` returns `None`. A missed
  lookup is recorded as `Absent`, never omitted, so creating a file at that
  path later changes the digest and invalidates the entry. A `Names` answer
  that is empty is likewise the digest of the empty set, not a missing read.
- `Names(kind, name)`: a digest of the sorted answer to a name-keyed question,
  for example the functions named `N`, the tests that call `N`, or the macro
  binding sites that bind `N`. A change elsewhere in the workspace that does
  not change that answer does not invalidate the probe.
- `Universe(kind)`: a digest of an entire fact set, taken by any scan that is
  not name-keyed. Any edit that changes that set invalidates every probe that
  holds this read.
- `Disk(path)`: the content hash of a file read directly from disk, or
  `Absent`, and for a directory walk the digest of the sorted
  `(path, content hash)` listing it saw. These reads bypass the index, so the
  index accessors cannot see them. Each one is wrapped in a recording read
  helper, and until it is, it calls `unrecorded()`. Phase A reports them.
- `Scalar(name)`: a digest of one manifest, config or module-structure field.

`Universe` is the fail-closed floor. A scan whose result depends on which
elements exist is recorded as a read of all of them. The design never narrows a
`Universe` read to a `Names` read by assertion: a call site is converted by
replacing the scan with a keyed lookup whose answer is the digest.

## Recording

The ledger is a thread-local scope opened around one probe's classification
and closed after it.

- Classification runs on the rayon pool, one probe per worker. Nested fan-out
  inside classification is refused (`nested_parallelism_allowed`), so every
  read a probe makes happens on its own thread and a thread-local scope sees
  all of them. The one exception, the macro-scan priming
  (`prime_workspace_macro_scan`, #9211), runs before any probe is queued and
  builds a run-scoped memo; the probe records its read of the memo, not the
  build.
- Run-scoped memos are shared across probes, so the build of a memo by probe A
  must not hide it from probe B. **Memos record on read, not on build.** Each
  memo accessor notes `(memo, key, digest of the answer)` into the active scope
  whether it hits or builds.
- The read paths into the index are `functions()`, `tests()`, `files()` and the
  `pub(crate)` fields above. The fields become private to `facts` and are read
  through accessors that note a read when a scope is active. A new read added
  without an accessor does not compile; a new accessor must pick a dependency
  kind.
- Any code path that cannot name its dependency calls `ledger.unrecorded()`.
  A scope that saw one is **not storable**: the probe is classified every run.

## Validation

A store entry is keyed by `probe identity + analyzer environment` and holds
`(ledger, finding, value_origins delta)`. The ledger is the list of
`(read, digest)` it recorded.

A lookup recomputes each read's digest against the current index and reuses the
entry only if every digest is equal. This is the discovered-inputs scheme of a
build system's action cache: the key cannot name the inputs because the
classification discovers them, so the entry carries them and is checked
against them.

Validation must cost much less than classification. `File` and `Scalar` reads
are a hash lookup. `Names` reads need a per-run inverted index (name to
functions, tests, call sites, macro sites), built from file facts that are
already cached per file. `Universe` reads are one digest per kind per run,
computed once.

The content hash is the file's content hash, as the corpus fingerprint uses
for the aggregate key. Mtime or stat data is not trusted for validity.

## Fail-closed rules

1. No stored entry without a closed ledger. An `unrecorded()` read, a panic, a
   cancelled run, or a refused budget leaves nothing stored.
2. A missing, corrupt or schema-mismatched entry is a miss, never an error,
   as every other cache layer here.
3. A digest that cannot be computed (unreadable file, non-UTF-8 source) is
   unequal to every stored digest.
4. Only facts are stored: the finding and the origins delta. Rendered output,
   diagnostics and hover text are not, as the seam cache rule already states.
5. The layer is registered in `cache_layers!`, so `cache clear` removes it and
   `ripr doctor` probes its directory, and `CACHE_SCHEMA_VERSION` moves when the
   finding layout does.
6. Actionability is not cached separately. The actionable flip stays a pure
   function of the finding; the cache returns the finding the shared validator
   would have produced, and verification mode (below) is the evidence of that.

## Proof

- **Differential oracle.** `RIPR_PROBE_CACHE=verify` recomputes every hit and
  fails the run on any byte difference in the finding or the origins delta.
  The check is run over the verdict corpus and this repository's own diffs.
  This is acceptance item 2 stated as a gate: a warm run is byte-identical to a
  cold run. The classifier is not the last writer of the report: the Rust
  adapter adds reach limits, source currentness and gap identities after it.
  The gate therefore also compares the final cold and warm reports byte for
  byte, not only the classifier findings.
- **One negative experiment per read kind.** Each experiment changes exactly
  one fact class and asserts the probe is reclassified: add a same-name
  function in another file; add a macro site that binds a name the probe calls;
  change a manifest `package_names` entry; edit a test the probe related; edit
  a test it did not relate and assert reuse. A read kind with no experiment that
  fails when its recording is removed is not covered.
- **Reuse is also asserted.** A one-file product edit must leave a stated
  number of unrelated probes as hits, or the layer can pass every correctness
  check by missing every time.

## Phases

**A. Measure; no cache.** Record the ledger behind `RIPR_PROBE_READ_TRACE` and
write, for a diff, each probe's reads grouped by kind and the call sites that
took `Universe` reads, with counts. Run it on this repository's `crates/ripr`
diffs (the 49 to 87 s case), on the cargo and tokio pins and on the verdict
corpus. The output is the share of probes that hold no `Universe` read, and the
`Universe` sites in descending probe count. This phase changes no result and
settles whether B and C pay: if almost every probe holds a `Universe` read on
the dominant scans, the layer would invalidate on nearly every edit and the
right next step is converting those scans, not caching.

**B. Convert the dominant `Universe` scans to `Names` reads**, one site per
change, each with the same-diff A/B and byte-identical reports that the
performance rule already requires. These are improvements without a cache.

**C. The store, with `verify` mode on in CI before it is on by default.**

## First implementation: index-read observations

Set `RIPR_PROBE_READ_TRACE=1` before starting the process to enable bounded
stderr JSON lines with the prefix `ripr_probe_read_trace` followed by one space. Presence enables the
trace, like the existing latency-trace knob; the process reads this setting
once. No new command or result cache is introduced.

Each `classify_probe_with_candidate_index_and_origins` invocation owns its
recording scope, including when one Rayon worker handles several probes.
`IndexFiles::get/get_key_value` record `file` or `absent`; `iter/values` and
file-set length record `universe(files)`; `functions/tests` record their
conservative whole-set views, while `RustIndex::test_at` reads one test fact by
position and records a `test` read named `file:start_line:name` instead. That
keyed read names the fact only: what the fact was derived from (for a
macro-generated test, definitions in other files) is still unsealed. A view is a whole-set dependency even when the
caller later narrows it. The site names the calling Rust source location.

Rows contain a probe id when representable, reads grouped by kind/subject/site
and count, `universe_observed`, and observed/dropped event counts. Repeated
reads at one site are counted. Identity and subject text is retained only when
UTF-8 and at most 512 bytes; it is never truncated into another path. At most
64 distinct read entries are retained per invocation. The JSON body is capped
at 32 KiB; if its escaped encoding exceeds that bound, all read detail is
omitted with an explicit flag and dropped-event count. The whole-set flag
survives either bound. A failed stderr write may leave missing or incomplete
trace data; it does not change analysis or panic. Reject an incomplete JSON
line rather than counting it as a finished observation.

Every row reports `coverage: "unknown"`, `reusable: false`, and
`analysis_completion: "not_established"`. This first increment does not seal
memo inputs, scalar fields, disk reads or dependency digests, and it does not
propagate recording into nested worker work. A missing
`universe` entry is therefore not a closed dependency set or evidence of reuse
eligibility. `classification_returned` describes only that function returning;
`interrupted` names an observed cancellation flag. Neither is proof that the
whole analysis completed or published a result. A forcibly stopped process may
emit only some observations, or none; it supplies no complete denominator.

The useful first measurement is a lower bound on observed whole-set reads and
their sites. Preserve unknown and dropped-detail counts. Do not use these rows
to report the complete no-whole-set-read fraction, claim invalidation or enable
Phase C. Remaining Phase A work must record memo dependencies on every read
(including warm hits and pre-primed answers), replace direct scalar access with
recording accessors, and cover or explicitly refuse the named disk inputs.

The controls compare actual traced and untraced findings plus private
`ValueOrigins`, exercise real index hits/misses and whole-set views, isolate
invocations and workers, and require a real nonempty CLI result with identical
stdout. Native completion and semantic parity are separate from a performance
measurement; no speedup is claimed by this diagnostic increment.

## Not in scope

- Reuse across analyzer versions, schema versions or cfg and feature sets. They
  are in the key.
- A persistent macro-scan cache. #9784 records the decision: at most about a
  quarter of CPU. Per-probe reuse subsumes the part of it that matters, because
  the scan is read through a memo and recorded as one.
- Language adapters other than Rust. Their classification does not share this
  index.
- Any speedup figure. None is claimed before Phase A.
