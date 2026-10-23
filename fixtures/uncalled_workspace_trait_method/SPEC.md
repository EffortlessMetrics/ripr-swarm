# Fixture: uncalled_workspace_trait_method

Spec: RIPR-SPEC-0001

## Given

`Inc::process` implements a trait declared in this workspace. No test calls
`process`, directly or through a helper. A same-file test asserts exact values of
an unrelated function `other`.

## When

The error literal in `process` changes and the public analyzer reads the
fixture.

## Then

The unrelated test only shares the file, and a workspace-trait method is
dispatched only through a call that names it, so no static test path is seen:
the finding is `no_static_path`, with no oracle-strength claim (#9169).

## Must Not

- Report `weakly_exposed` or a strong oracle borrowed from `other`'s tests.
- Treat a workspace-trait impl method as reachable by unseen dispatch merely
  because it is a trait method.
