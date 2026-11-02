# Kani native export contract fixtures

The two upstream JSON inputs are byte-identical published schema `0.1.0` contract fixtures. They are not claimed to be captured verifier runs or evidence of proof validity.

The parser and failure fixture are pinned to [`model-checking/kani@744b3e84d6eb68fd5456826997568666791bd355`](https://github.com/model-checking/kani/tree/744b3e84d6eb68fd5456826997568666791bd355), especially `kani-driver/src/frontend/schema_utils.rs`. The success fixture comes from the export author's [`ivmat/kani@47c2b9921374a99091c0bd48a3e8e19e92f8d8dd`](https://github.com/ivmat/kani/tree/47c2b9921374a99091c0bd48a3e8e19e92f8d8dd). Both source files are in `tests/json-handler/schema-validation`; their Git blob identities were verified when vendoring.

| Vendored file | Source filename | Git blob SHA |
| --- | --- | --- |
| `fork-completed-success.json` | `fixture-completed-success.json` | `d0652850e8594ee65578320571d8ceca672580bf` |
| `completed-failure.json` | `fixture-completed-failure.json` | `4385be09b79e17b3881c8888b11b72701f890ff9` |

`native/tests.rs::resultless` constructs empty and resource-failure controls from the success fixture's metadata. It replaces outcomes, removes property results, makes unavailable counts null, and supplies explicit selection and summary accounting. These authored inputs exercise timeout, out-of-memory, crash, partial results, and no selected harnesses; they are not verbatim upstream exports. Other mutations test expected-panic polarity and malformed inputs.

`COMPLETE` describes accounting. `SUCCESS` is a native harness verdict and may coexist with unreachable checks or expected assertion failures. Successful checks generally have only a count; the success fixture also carries a successful unsupported-construct record with an ID. The failure fixture exercises contracts/stubs, unsupported failure, covers, warnings and `other` statuses.

The published `kani-0.68.0` release predates this export profile and uses a different JSON shape. Its version string alone does not select the profile. Native qualification still requires archived runs of the pinned exporter, commands/process outcomes and bound source/configuration; these fixtures do not establish it.

Original fixture copyright: Kani Contributors; upstream license: `Apache-2.0 OR MIT`. See [`LICENSE-APACHE`](https://github.com/model-checking/kani/blob/744b3e84d6eb68fd5456826997568666791bd355/LICENSE-APACHE) and [`LICENSE-MIT`](https://github.com/model-checking/kani/blob/744b3e84d6eb68fd5456826997568666791bd355/LICENSE-MIT).

The separate [native conformance subset](native-captures/README.md) contains eight
genuine untouched exports, their exact source and a portable provenance projection.
It supplements these contract examples with offline replay controls, retaining
their distinct origin and the remaining profile-qualification limits.
