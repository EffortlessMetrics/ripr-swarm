# Canonical native binding and input preparation

`formal-claim-registry prepare-native` joins native declarations to the one
canonical registry in `policy/formal-claims.json`. It does not execute Kani.
Exit zero means that passive preparation completed; the output may contain
missing bindings or native cover presentations. The canonical scoreboard is
rendered by the existing registry owner and remains non-established.

## Run it

```sh
cargo run --locked -p xtask --no-default-features \
  --features formal-native-bindings --bin formal-claim-registry -- \
  prepare-native --root .
```

Without `--bindings`, every registered claim is reported with its exact ID,
generation, decision digest, subjects, required covers, negative controls, and
missing native binding. This is a useful inventory of remaining obligations,
not a reduced denominator or a passing native result.

Add `--bindings path/to/declarations.json` to prepare reviewed native locators
and source inputs. The input contract is `ripr.native-bindings.v1`:

| Field | Meaning |
| --- | --- |
| `schema` | Exactly `ripr.native-bindings.v1`. |
| `registry_generation` | The canonical registry's complete numeric/digest identity. |
| `bindings` | Zero or more exact canonical claim bindings; unknown and duplicate IDs refuse. |

Each binding has `claim_id`, `claim_generation`, `decision_digest`, `profile`,
`subjects`, `covers`, `correspondence`, and a nonempty `limitations` array. The
canonical claim document continues to own the proposition, semantic subject,
domain, assumptions, bounds, required covers, and negative controls. Changing
any of those decisions requires the existing canonical generation transition;
a provider binding cannot supply replacement semantics.

The declared ordinary Kani profile has `kani_version: "0.68.0"`,
`cbmc_version: "6.11.0"`, `target: "x86_64-unknown-linux-gnu"`,
`configuration_policy: "kani-0.68-defaults-v1"`, `default_unwind: 32`, `lib: true`,
`default_features: true`, `features: []`, and `all_features: false`.
These are requested configuration values. They do not establish an installed
compiler or observed native invocation.

The separately declared `kani-0.68-rust-only-v1` profile keeps the same pinned
tools, target, library target and unwind default, and requires exactly
`default_features: false`, `features: ["lang-rust"]`, `all_features: false`.
Source review of the five selected canonical proof routes supports this
candidate configuration: their selected bodies have no language-feature
branches and both manifests expose the Rust-only route. This does not establish
default-feature equivalence or native results for those claims. The policy name
and binding digest distinguish the profiles; retained arguments and resolved
Cargo metadata must separately match before a future native observation join.
Other feature sets refuse until a separately reviewed policy is implemented.

Every subject binding supplies the unchanged `canonical_subject`, the package
(`ripr` or `ripr-contract`), a file in the canonical source closure, and the
fully qualified native function name. The bare function name must agree with
the canonical subject. The source scanner checks the ordinary proof attribute
and exactly the optional canonical local unwind override. Thus a claim with
`bounds.unwind: 16` keeps effective unwind 16 while the provider default remains
32. The preparation does not prove Rust module resolution or the semantic
adequacy of the declared source correspondence.

A cover binding supplies the canonical `obligation`, `canonical_subject`,
source `file`, unique literal `anchor`, and nullable `presentation`. A native
presentation supplies `property`, exact quoted `description`, `file`,
`function`, positive `line`, and positive `column`. Files must belong to the
canonical semantic closure. The anchor and location are checked against the
bytes being committed. Property identities and description/location
presentations cannot be reused for two cover obligations in one subject.
Missing mappings or nullable presentations remain visible as missing
obligations; the required cover inventory never shrinks to the supplied list.

The canonical currentness claim now declares all six required covers: four
constructor witnesses and two semantic predicates. Its explicit claim
generation 1-to-2 and registry generation 2-to-3 transition preserves the
canonical ID. The binding example below uses that exact decision; it does not
alias a historical aggregate claim or reuse historical result authority.

Unknown JSON fields and duplicate object keys refuse. Flags such as
`native_captured`, `source_current`, or `execution_authority` cannot construct
a validated registry or a preparation capability.

## Source policy and limits

`ripr-workspace-ordinary-sources-v1` starts with the canonical claim closure,
claim/global authority files, the exact registry/claim document bytes, workspace
and package Cargo files, the Rust toolchain file, process/cancellation owners,
and all files under `.cargo`, `xtask/src`, and `tools/repo-policy`. It includes
the selected package source tree and `ripr-contract` sources when selecting
`ripr`. The shared `formal_source_inputs` owner then extends this set for
supported literal inputs, path attributes, and default build scripts.

Every source path is repository-relative and every component must be a regular
directory or file, with symlinks refused. Individual source-cache reads are
limited to 8 MiB per file; the retained cache is limited to 256 MiB. The shared
scanner also performs bounded file reads, so 256 MiB is not a cumulative I/O
limit. Inventory/traversal limits are 20,000 entries and directory depth 128.
The registry and binding documents retain their 1 MiB per-file limit. Native
subject and cover checks consume the same cached bytes that are hashed into
the report. Canonical membership and input bytes are readmitted by the existing
registry loader and checked against its validated snapshot before and after
preparation; a newly added unlisted claim cannot evade the check by leaving
previous file hashes unchanged.

This is a conservative local input observation. It does not discover arbitrary
macro expansion, external dependency contents, arbitrary build-script I/O, or
every compiler input. It does not establish atomic filesystem custody,
compilation, native process execution, production conformance, or a protected
lifecycle. Unsupported visible source forms refuse through the shared scanner.

## API and authority boundary

`formal_registry::ValidatedRegistry::load(root)` is the only constructor for
the read-only canonical snapshot. Public borrowed claim views expose the
canonical identities and ordinary claim documents. `prepare_native` returns
an opaque `PreparedNativeInputs` value. Its JSON is a report and cannot be
deserialized back into an authority-bearing handle.

The handle and borrowed claim views are immutable observations at preparation
time. A live capture must repeat preparation before and after execution; a
borrowed view is not continuously current source authority. The
[passive replay route](NATIVE_REPLAY.md) reproduces retained interpretation
without constructing a live process observation or promoting assurance.

The report includes the unchanged canonical scoreboard, every expected claim,
binding state, missing obligations, source manifests and digests, explicit
inputs, canonical-to-native subject correspondence, and binding declarations.
`native_origin` stays `not_established`, `native_assurance` stays `not_assessed`,
and `execution_authority` stays `not_granted`. Retained-byte replay, even with
matching recomputed hashes, cannot change those authority boundaries. The
protected #14500 provenance/currentness join and #6452/#6456 transitions remain
external prerequisites; #14469 continues to own complete positive-cover
admission in the live lane.

## Document contracts and a real binding

The [native binding schema](schema/native-bindings-v1.schema.json) describes
the untrusted input shape, including the two exact declared profiles. The
[preparation report schema](schema/prepared-native-inputs-v1.schema.json)
reuses the existing native-subject and canonical-scoreboard schema owners.
Schema validation is structural; exact canonical identity, membership,
generation, source, and cover semantics remain in the Rust registry/preparer.
Neither a schema-valid JSON document nor a recomputed digest reconstructs an
opaque validated handle or grants result authority.

The [currentness binding example](examples/currentness-native-bindings.example.json)
names the actual canonical currentness ID, short subject, native locator,
generation and six required cover presentations. Its native property metadata
comes from the retained `currentness-mutant.stdout` format fixture with matching
proof/domain source bytes; it claims no current positive execution. It selects
the separately declared Rust-only candidate profile.

After the constructor-domain transition, this example names registry generation
4 explicitly. Currentness remains at claim generation 2 with its six-cover
decision unchanged. This declaration update neither transfers the earlier
generation-3 receipts nor establishes a current native result; rerun preparation
against the composed tree before using it.

```sh
cargo run --locked -p xtask --no-default-features \
  --features formal-native-bindings --bin formal-claim-registry -- \
  prepare-native --root . \
  --bindings docs/formal-verification/examples/currentness-native-bindings.example.json
```

The example is deliberately tied to its canonical registry generation. After
an accepted registry transition, review the binding against the resulting
canonical decision, update its exact registry/claim identities as appropriate,
and rerun preparation and emitted-document schema validation. Do not silently
rebind old declarations or mark historical results current. Retain the complete
source manifest and native exit of a real bound preparation; a missing-bindings
report alone does not exercise the source scanner over the full closure.

## Maintenance and qualification

Keep canonical membership and decision digests in the registry owner. Keep
native property parsing, profile checks, and source expansion in their existing
shared owners. New profiles, source rules, and cover correspondence need direct
positive/refusal controls and a documented interpretation boundary.

Focused commands are:

```sh
cargo test --locked -p xtask --no-default-features \
  --features formal-native-bindings --lib
cargo clippy --locked -p xtask --no-default-features \
  --features formal-native-bindings --lib --bin formal-claim-registry -- -D warnings
```

Include the actual library test graph in strict Clippy qualification as well.
Required controls cover canonical key/generation/digest substitution, missing
denominators, duplicate cover witnesses, source/configuration refusal, changed
registry bytes, strict JSON, and nonpromotion of supplied evidence. Reversibly
remove the claim generation/digest guard and observe the intended key-refusal
test failure before restoring the exact source. Retain the native exit and
exact source identity; compilation alone does not establish that control.

The current implementation is prepared on an explicitly named multi-parent
integration of the registry, auditor, DTO/parser, source, and planner repairs.
That cumulative integration is not an admitted small PR. Owner-coordinated
composition and landing must precede bounded PR admission; measure this seam
against its actual integration parent and report the full cumulative size.
