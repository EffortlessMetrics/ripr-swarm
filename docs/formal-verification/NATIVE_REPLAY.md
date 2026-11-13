# Passive retained native replay

`formal-claim-registry replay-native` reproduces native text interpretation and
joins it to locally prepared canonical claim inputs. Exit zero means retained
consistency. It does not establish that the supplied process, compiler, solver,
source observation, or cleanup actually occurred.

```sh
cargo run --locked -p xtask --no-default-features \
  --features formal-native-bindings --bin formal-claim-registry -- \
  replay-native --root . --bindings path/to/bindings.json \
  --run path/to/native-run.json --artifacts path/to/retained-directory
```

The binding input follows [native preparation](NATIVE_BINDINGS.md). The run is
the existing `ripr.native-proof-run.v1` document, with empty registered results
and expected-claim IDs. Native observations are retained separately from
canonical assurance. A historical `formal-kani` artifact cannot be relabeled
into this contract: the complete retained inputs below must be supplied by a
compatible producer. This command executes no provider or captured arguments.

## Supported replay profile

The initial profile is exactly one package (`ripr` or `ripr-contract`), one fully
qualified harness selected with `--exact`, and `kani-0.68-rust-only-v1`:
Kani 0.68.0, CBMC 6.11.0, the pinned bundled compiler and target, `--lib`,
`--no-default-features --features lang-rust`, and default unwind 32. A canonical
local override, such as gate unwind 16, remains checked by preparation. The
default-feature profile remains a valid declaration for preparation but is not
accepted by this retained replay route.

| Retained input | Consistency checked by the shared Rust owner |
| --- | --- |
| Before/after source manifests | Hash/length commitments, captured file counts, source identity and stability declarations. |
| Four Git observations per complete snapshot | Exact arguments, success and raw revision/tree/status bytes. These are supplied observations, not authenticated Git custody. |
| Configuration snapshots | Every prepared source commitment, supported Cargo key structure and known locator commitments. This does not prove complete inherited environment discovery. |
| Cargo metadata | Exact selected manifest/workspace, reachable resolved graph and Rust-only features on local proof packages; successful metadata invocation and exact arguments. |
| Installed-toolchain records | Pinned compiler/target strings and the complete named binary-hash inventory. Supplied hashes do not authenticate installed executables. |
| Native inventory | Shared list parser, exact qualified selection, declared subjects and package input denominator. |
| Invocation streams and raw index | Regular-file/hash/length checks, supported arguments/directories/deadlines, process projection and command digest. |
| Native verification text | Shared property parser and verdict law, including unreachable, missing and incomplete covers. |
| Retained preparation | Exact JSON equality with a newly loader-constructed canonical preparation, including bindings and all expected claims. |
| Canonical cover mappings | Every required property, unique mapping, exact description and source location; source closure and explicit inputs must match local preparation. |

Each artifact path has one immutable verified byte snapshot within a replay.
Conflicting commitments for the same path refuse. The cache retains at most
96 MiB of unique artifact bytes; repeated references share the cached bytes.
This is a retained-byte budget, not a total process-memory or parsing-work
limit. At most twelve invocation records and one native package inventory are
accepted, bounding repeated interpretation. Binding and run input files retain
the CLI's 1 MiB limits. Parsing, owned result structures and local preparation
have additional memory/I/O costs described in the preparation runbook.

## Authority and currentness

The [consistency report schema](schema/retained-native-consistency-v1.schema.json)
reuses the existing native-subject, observation-status and canonical-scoreboard
definitions. Each canonical claim remains in the report, including missing
bindings, missing presentations, missing observations and negative-control
obligations. An observed `VERIFIED` text interpretation is separate from the
canonical scoreboard. All canonical statuses remain in their baseline state;
`native_origin` is `not_established`, `native_assurance` is `not_assessed`, and
`execution_authority` is `not_granted`.

`PreparedNativeInputs` and its borrowed claim views are immutable observations
at preparation time. They are not continuously current source handles. A live
producer must prepare before and after execution and compare the actual
observations. Passive replay can check supplied bytes against its snapshot;
it cannot establish that a compiler consumed those bytes or detect every
intervening filesystem change. Atomic custody, arbitrary compiler/build-script
dependencies and protected provenance are outside this route.

The actual process owner must separately provide an unforgeable local execution
observation. Neither a deserialized `Run`, supplied producer names, successful
exit fields, byte hashes nor this replay result can create that capability.
Even a real local observation does not itself grant protected #14500 provenance
or #6452/#6456 lifecycle/transition admission. Complete positive-cover admission
continues to use the shared law and the composed #14469 live-lane witnesses.

The committed currentness example is explicitly updated to registry generation
4 after the constructor-domain transition. Its own six-cover decision remains
at claim generation 2 with the same decision digest. The example requires fresh
preparation against the resulting source/configuration closure. The original
replay qualification used registry generation 3; those run/configuration/source
receipts retain that identity and cannot be relabeled. Every later global
registry transition likewise requires an explicit binding update, even when
the individual claim's generation/digest is unchanged.

## Required controls and maintenance

Run the focused commands in [native preparation](NATIVE_BINDINGS.md), including
the actual library test graph under strict Clippy. The replay controls create a
fully self-consistent supplied bundle that even fabricates success for
`cover!(false)`. Both the initial and recomputed bundle must replay successfully
while every assurance/origin/authority field remains unestablished. A malformed
bundle refusal alone does not establish this boundary. A reversible mutation
of the fixed origin projection must make this exact control fail after it
compiles; retain the native red/restored exits separately.

Other controls cover altered argv, corrupt artifacts, recomputed source
omission, recomputed Cargo features, substituted canonical preparation, changed
cover presentations, foreign registry identity, substring selection, public
CLI behavior and artifact-cache/resource limits. Validate actual emitted JSON
against the local schema resources, refusing unknown resource IDs. Schemas
describe structure; the existing Rust semantic owners remain authoritative.

Maintain this as a bounded seam over its named ordinary integration parent.
The full registry/source/parser integration remains separately measurable and
is not an admitted small aggregate PR or CI request.
