# Static Limits

Static limits explain what RIPR could not safely infer from syntax-first
evidence.

They are part of the evidence, not a separate verdict. A preview finding can
still point at a useful related test, assertion shape, or focused next action,
but the static limit must stay visible before anyone acts on that evidence.

## Where Static Limits Appear

Static limits can appear in:

- JSON findings as `static_limit_kind`;
- human and generated-CI summaries as static-limit text;
- VS Code diagnostics and hover/status text;
- agent packets and briefs when the selected finding carries the limit.

The stable `static_limit_kind` values are:

```text
dynamic_dispatch
metaprogramming
missing_import_graph
decorator_indirection
mocked_module
opaque_custom_assertion_helper
property_based_test
unresolved_pytest_fixture
unsupported_syntax
cross_language_oracle_visibility_unresolved
rust_transitive_reach_unresolved
rust_integration_public_api_path_unresolved
rust_macro_reach_unresolved
rust_macro_wrapped_test_call_unresolved
rust_macro_wrapped_assertion_unresolved
rust_value_propagation_unresolved
rust_subprocess_binary_reach_unresolved
wrapper_error_binding_unresolved
python_transitive_reach_unresolved
```

When `static_limit_kind` is absent but stable static-limit text is present,
render the text as evidence. Do not parse that prose to invent a different
action.

## How To Read Each Kind

| Kind | Plain-language meaning | What to do with it |
| --- | --- | --- |
| `dynamic_dispatch` | The call target or behavior may be selected dynamically, such as computed member calls (`obj[name]` followed by invocation) or `getattr(obj, name)(...)`. | Treat the finding as advisory. Prefer a focused test that observes the concrete runtime target or result. |
| `metaprogramming` | The code shape may change behavior through a metaprogramming mechanism, such as proxies, metaclasses, generated attributes, or similar indirection. | Keep the limit visible. Do not assume the static owner or call path is the full runtime boundary. |
| `missing_import_graph` | The preview adapter did not resolve a full project import graph. | Check whether the related test and owner are the intended files before copying a packet or opening a test. |
| `decorator_indirection` | A Python decorator may change the callable boundary before the body runs. Simple route decorators such as `@api.post(...)` can still be route metadata when the changed body has a supported repair shape. | Treat owner/test evidence as syntax-first. Add a test around the decorated public behavior, not only the undecorated body. |
| `mocked_module` | A test replaces or mocks a module or symbol involved in the finding, such as `unittest.mock.patch(...)` or pytest `monkeypatch.setattr(...)`. | Read the mock as interaction evidence, not proof of the real dependency behavior. Keep repair routing blocked unless a separate non-mocked concrete oracle path exists. |
| `opaque_custom_assertion_helper` | A related Python test observes behavior through a custom assertion helper whose body is not inspected. | Keep the finding out of repair queues until a human or analyzer can confirm whether the helper already observes the changed discriminator. |
| `property_based_test` | A related Python test uses generated inputs, such as Hypothesis `@given(...)`, and syntax alone cannot prove which concrete examples run. | Do not assume the generated cases include the missing discriminator. Keep repair routing blocked unless that same related test also contains concrete strong oracle evidence. |
| `unresolved_pytest_fixture` | A related pytest test depends on fixture-sourced values that the preview adapter does not execute or resolve. | Do not assume the fixture supplies the missing discriminator or expected value. Keep repair routing blocked unless a separate concrete oracle path exists. |
| `unsupported_syntax` | The parser or preview adapter saw syntax outside the current preview contract. | Do not upgrade the finding into a stronger claim. Use the packet as a pointer for manual inspection. |
| `cross_language_oracle_visibility_unresolved` | The changed Rust seam is exposed across a language boundary, but RIPR cannot statically tell whether the external-language oracle discriminates the change. | Verify the external oracle directly. Do not convert the finding into a Rust repair packet from this label alone. |
| `rust_transitive_reach_unresolved` | A Rust test appears to call an entry point that may lead toward the changed owner through a transitive helper path RIPR does not fully trace. | Treat this as a named `no_static_path` limitation. Inspect the candidate path before adding or delegating repair work. |
| `rust_integration_public_api_path_unresolved` | An integration test appears to call crate public API, or a test helper that calls it, along a candidate path toward the changed owner. | Treat this as first-run evidence that RIPR saw the integration test but could not cross the public-API path. It is not a clean result or a reach claim. |
| `rust_macro_reach_unresolved` | A Rust test appears to call an entry point whose path toward the changed owner stops at a same-repo macro invocation RIPR does not expand. | Inspect the macro path manually or improve macro-aware reach. Do not treat the macro mention as evidence that the test observes the change. |
| `rust_macro_wrapped_test_call_unresolved` | A Rust test directly invokes a same-repo macro whose definition mentions the changed owner, but RIPR does not expand that macro. | Treat this as first-run evidence that RIPR saw the test-body macro call but could not cross it. It is not a clean result, reach claim, or repair packet. |
| `rust_macro_wrapped_assertion_unresolved` | A Rust test reaches the changed owner, but the visible assertion-like custom macro is not classified as an oracle. | Treat this as first-run evidence that RIPR saw a candidate assertion macro but could not confirm its discriminator. It is not a clean result, oracle claim, or repair packet. |
| `rust_value_propagation_unresolved` | A changed Rust `let` binding uses a bounded `find`/`rfind` or `len_utf8` operation normalized through `map_or`, but syntax-first analysis cannot carry that value into a same-owner equality predicate. | Treat this as a named `static_unknown` analyzer limitation. Do not add a duplicate discriminator test or infer coverage, repair readiness, or a runtime result. |
| `rust_subprocess_binary_reach_unresolved` | An integration test invokes a Cargo-built binary, but ripr does not yet map that executable back to the changed owner. | Treat this as a named `no_static_path` limitation. Inspect the subprocess test and binary target manually; do not infer reach, receipt validity, coverage, or repair readiness. |
| `wrapper_error_binding_unresolved` | A wrapper error conversion (`callee(..).map_err(..)`) takes its error-variant identity from the converted callee, and RIPR cannot establish that the boxed conversion preserves that variant. | Keep the seam below `exposed`; verify the variant through the wrapper directly. This names the unresolved conversion binding, not a coverage or repair claim. |
| `python_transitive_reach_unresolved` | A Python test constructs or calls into the owner's class, and a bounded same-class method path may lead toward the changed method. | Treat this as a named `no_static_path` limitation. Inspect the candidate class/method path before adding or delegating repair work. It is not a related-test or coverage claim. |

## Seam Readings: `opaque` and `activation_unknown`

Repo-wide output (`ripr pilot`, `ripr check --format repo-exposure-json`, and
the editor's seam diagnostics) grades every seam with a grip class. Two of the
classes mean ripr stopped short of a verdict about the tests. Human output
prints both with the plain word `unknown`, for example `unknown, opaque`.

| Reading | What ripr found | What it could not establish | What to do |
| --- | --- | --- | --- |
| `opaque` | A candidate path to the seam that ripr does not trace: a test calling a public function that calls a private helper, a macro, or a trait impl that a test or test-reachable function mentions by type and may run through a call such as `to_string()`. The seam's `opaque_static_evidence` limitation names the test or production function and the path. No related test is established. | Whether any test runs the seam, and if one does, whether it notices a wrong value there. | Do not write a test from this reading alone. If the limitation names a test, open it and check that it exercises the seam and has an assertion that would notice a wrong value there. If it names a production function instead, as trait-dispatch evidence can, find the tests that call that function and check the same. If such a test exists and asserts on the value, leave the seam. If no test runs the seam, the test only runs the code, or it asserts on something else, add a direct test of the owner. |
| `activation_unknown` | Activation is not established: ripr has no input value that reaches the changed behavior. A related test is listed, but it may be linked only by name or file, so ripr has not shown that it calls the seam's owner. A boundary-value hint may still be listed under the seam's missing discriminators; read it as guidance on where to look, not as a confirmed gap. | Whether the listed test calls the owner (limitations `activation_owner_call_absent` and `activation_owner_call_unresolved`), and which input it passes. A value that comes through a helper, fixture, environment variable or computed local records a value limitation: `activation_boundary_input_unresolved` when a local, iterator, closure or computed operand feeds the boundary, `activation_value_unresolved` when no literal value was observed. | First read the seam's limitation category. If it is `constructor_field_owner_ambiguous`, an exact field observer already exists and ripr could not choose between same-named owners: do not add a test that bypasses the public path, and file an analyzer follow-up (the repair route is `analysis/constructor-field-observation`). Otherwise open the related test. If it never calls the owner, add that call or a direct test. If it does, read the value it passes. If it lands on the changed boundary, also check that its assertion would notice a wrong value there; once activation is established, a weak assertion shows as `weakly_gripped`. If the assertion is exact, there is nothing to add. If the value is already a known literal or constant that ripr could not follow, state it as a literal or a same-file constant at the call. If it comes from a helper, fixture, environment variable or computed local, keep the test input unchanged and trace where the value comes from by hand. |

How the two count:

- `opaque` is not headline-eligible, so it stays out of the headline gap count,
  and `ripr pilot` lists it after every other class. In the editor its
  diagnostic severity is `[severity.seams] opaque`, default `info`.
- `activation_unknown` is headline-eligible. In the editor its severity is
  `[severity.seams] activation_unknown`, default `info`.
- `ungripped` (`no path` in human output) means ripr found no related test and
  no unresolved candidate path. A seam with an unresolved path reads `opaque`, not `ungripped`.

`opaque` errs toward unknown. Matching is by name, so a trait impl of a type
that your tests use reads `opaque` even when no test runs that impl (`Debug`,
`Hash` and `Drop` impls are typical). `opaque` is a prompt to look, not
evidence that a test exists. RIPR-SPEC-0230 lists the limits.

## External-Language Related-Test Inventory

Which related-test files count as external-language evidence is read from the
routed TypeScript/JavaScript extension authority in
`crates/ripr/src/analysis/language/router.rs`
(`TYPESCRIPT_SOURCE_EXTENSIONS` / `JAVASCRIPT_SOURCE_EXTENSIONS`), the same
owner the language router dispatches on. The fail-closed cross-language gate
(`analysis::repair_route`) and the navigation-only external observer projection
(`output/agent_seam_packets.rs`) both consume it, so a routed extension such as
`.mts` or `.cts` cannot be analyzed as TypeScript while its test is still
treated as an unknown language.

| Extension | Published external-language label |
| --- | --- |
| `.ts`, `.tsx`, `.mts`, `.cts` | `typescript` |
| `.js`, `.jsx`, `.mjs`, `.cjs` | `javascript` |
| `.py` | `python` |
| `.rb` | `ruby` |
| `.java` | `java` |
| `.c`, `.cc`, `.cpp`, `.cxx`, `.swift`, `.kt`, `.kts` | none; still external evidence in the repair route, so the limitation stays unresolved |

Admitting an extension never promotes a finding. It only keeps the
`cross_language_oracle_visibility_unresolved` gate and the navigation-only
external observer label in step with the router. Matching is exact on the
case-folded extension, so a near-miss suffix such as `.mtsx` or `.ctsx` is
unknown rather than external. The bridge languages above remain a repair-route
list: they are external evidence without a routed adapter or a published
label.

## What Static Limits Do Not Mean

A static limit does not mean:

- runtime mutation testing ran;
- coverage or runtime adequacy was established;
- the preview finding is policy-eligible by default;
- the editor can edit source or generate a test;
- the adapter is claiming Rust-level maturity;
- the action should change merely because a limit string appeared.

It means RIPR found enough syntax-first evidence to show a bounded preview
finding, while naming the part it could not model safely.

## Editor Read Order

For preview evidence, read editor output in this order:

```text
language
preview status
static limit
observed evidence
bounded next action
```

If the status is stale, wrong-root, malformed, disabled, or unavailable, that
state dominates. The editor should not project preview action text from stale or
untrusted artifacts.

## JSON And Integrations

Use `static_limit_kind` as a structured display and grouping field. It is safe
to group, count, and label by this value.

Do not branch code-action semantics on parsed human text. Text-only static
limits are display evidence until a structured kind exists.

For tool output contracts, see [Output schema](OUTPUT_SCHEMA.md). For the
editor projection rule, see
[RIPR-SPEC-0037: Editor preview static-limit projection](specs/RIPR-SPEC-0037-editor-preview-static-limit-projection.md).

## Related Docs

- [Language adapter preview workflow](LANGUAGE_ADAPTER_PREVIEW.md)
- [Editor extension](EDITOR_EXTENSION.md)
- [Support tiers](status/SUPPORT_TIERS.md)
- [TypeScript preview static facts](specs/RIPR-SPEC-0027-typescript-preview-static-facts.md)
- [Python preview static facts](specs/RIPR-SPEC-0028-python-preview-static-facts.md)
