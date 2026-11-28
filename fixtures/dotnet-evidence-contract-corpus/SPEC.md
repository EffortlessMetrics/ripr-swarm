# Fixture: dotnet-evidence-contract-corpus

Spec: RIPR-SPEC-0393

This manifest-only fixture pins normative acceptance shapes for the .NET shared
project substrate and separate C#, F#, and Visual Basic evidence contracts
(#5632) before adapter implementation (#5633–#5645).

## Given

A corpus case describing project facts, language-scoped semantic identities,
test activation/oracle ordering, or limitation stops under the contracts in
`docs/specs/RIPR-SPEC-0393-dotnet-shared-project-and-language-evidence-contracts.md`.

## When

Maintainers or future xtask checks compare a case's `expect` field to the
normative rules in RIPR-SPEC-0393.

## Then

`accept` cases align with the spec's Behavior and Acceptance Examples.
`reject` cases document forbidden promotions (for example synthetic
`language = dotnet` semantic findings or universal TFM claims).

## Must Not

- Execute MSBuild, NuGet restore, or test hosts.
- Collapse C#, F#, or Visual Basic identities.
- Treat project references as changed-owner reach without registered edges.

Run (future, when wired in #5634):

```bash
cargo xtask check-fixture-contracts
```
