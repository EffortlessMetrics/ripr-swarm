# Fixture: typescript_then_function_scoped_var_shadow

Spec: RIPR-SPEC-0246 rule 2; RIPR-SPEC-0108

## Given

The test returns the real owner's promise, but its callback declares a nested
block `var total`. That binding belongs to the callback function, so the later
assertion calls the fake total outside the nested braces. Actual Vitest 5.0.5
controls pass for both expected and wrong real-owner returns. `const` and `let`
controls using the resolved value fail for the wrong return.

## When

Run `ripr check --root input --diff ../diff.patch --format json --mode fast`.
The native typescript_implicit_promise_return integration target contrasts
zero/one parameters, TS/JS, returned/awaited/implicit promises, nested const/let,
var in separate nested functions and function-declaration hoisting.

## Then

One changed return-value finding remains `static_unknown`, with an unknown
oracle on the related test. Function-scoped var cannot be proved independent
using a lexical block interval. The existing AST hoist walk guards this promise
callback while skipping other function bodies. The independent honesty case
forbids false promotion even if a wrong golden is blessed.

## Must Not

- Credit the local fake assertion as a discriminator of the imported owner.
- Suppress supported const/let or independent nested-arrow-var controls.
- Infer general function scoping from the shared lexical scanner, or claim
  broader promise, alias, sloppy-mode function or var dataflow semantics.
