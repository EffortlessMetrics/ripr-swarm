# unsafe-review lane

`unsafe-review` is the advisory unsafe-contract review lane for repositories
that contain unsafe, FFI, raw pointer, layout-sensitive, GPU/native, parser, or
C ABI seams.

It answers a different question from a source exception ledger:

| Tool | Question |
| --- | --- |
| `cargo-allow` | Is this unsafe or source exception allowed and owned? |
| `unsafe-review` | Is this unsafe seam reviewable: contract, guard, test reach, and witness route? |
| Miri / sanitizers | Did a selected concrete execution expose UB or memory misuse? |

This repository keeps `unsafe_code = "forbid"` for production Rust. The lane is
therefore a doctrine and integration point for future or downstream unsafe
surfaces, not a license to add unsafe code.

## Claims

`unsafe-review` is advisory reviewability evidence. It may say that a changed
unsafe seam has or lacks an inspectable contract, local guard, test reach, or
witness route.

It does not prove memory safety or UB-free status. Concrete execution witnesses
come from Miri, sanitizers, focused tests, or other runtime tools and must be
attached as receipts when they are part of the claim.

## Default placement

```text
Default PR:   run only when changed paths or risk packs touch unsafe-capable seams
Risk PR:      require an unsafe-review card or explicit waiver for changed unsafe seams
Nightly/main: refresh broader unsafe reviewability reports when unsafe exists
Release:      require witness receipts for release-bearing unsafe contracts
```

## Artifacts

A mature integration should write reviewable artifacts such as:

```text
target/unsafe-review/
  cards.json
  pr-summary.md
  github-summary.md
  cards.sarif
  comment-plan.json
  witness-plan.md
  lsp.json
  receipt-audit.json
```

Artifacts should identify the seam, contract, local guard, static or runtime
reach, missing witness route, owner, and follow-up command. They should be
concise enough for a reviewer or coding agent to act without reading the whole
unsafe implementation first.

## Relationship to repo policy

`unsafe-review` should not replace exception ownership. If unsafe is ever
introduced, source-visible exceptions still need a durable owner and reason via
`cargo-allow` or the repository's active policy ledger. `unsafe-review` adds the
reviewability plane: what contract the exception relies on and how reviewers can
inspect or witness it.

## See also

- [`cost-and-verification-policy.md`](cost-and-verification-policy.md) — CI economics doctrine.
- [`test-evidence-lanes.md`](test-evidence-lanes.md) — static/runtime lane split.
- [`lem-budgeting.md`](lem-budgeting.md) — LEM planning unit and budget bands.
