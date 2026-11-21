# Patch presentation and style

Prefer one coherent patch for one demonstrated root cause, with the necessary
regression evidence. Follow the enrolled source's AGENTS.md and existing
semantic ownership. Preserve the public package/API, documented advisory and
unknown outcomes, and established feature boundaries unless the fix requires
an explicitly explained compatibility change.

Keep the production change focused. Avoid unrelated formatting, dependency
refreshes, broad crate extraction, report/schema expansion or new CI machinery.
Use the existing error, path, process, atomic-write and artifact-admission
abstractions where they own the behavior. Do not copy a security decision into
a renderer or test helper as a shortcut.

Include a discriminating regression test using the actual affected producer or
public entrypoint. Show the vulnerable behavior being rejected after the fix,
not merely a helper returning a constant or a source-text assertion. Include a
legitimate near-miss or recovery case to prevent a refuse-everything fix.
Preserve stale/wrong-root and other existing negative controls relevant to the
changed boundary. When an existing test is intentionally revised, explain why
the old expectation no longer describes the supported contract.

Keep Rust fallible and consistent with the repository's unsafe/panic policy.
Apply the pinned formatter. Explain platform-specific handling and preserve
literal argument boundaries for subprocesses. Do not weaken an assertion or
silence a required check merely to make the patch appear green.

The patch summary should identify the root cause, changed contract, affected
callers, tests actually run, tests not run, compatibility risk and rollback.
Retain exact source and test identities in the accompanying evidence. Label
unexecuted tests, incomplete image builds and missing native-platform proof
explicitly. Follow the repository's changelog-fragment convention when the
change affects product behavior or an operational contract.

This file governs patch style and evidence presentation. It does not select
security scope, alter severity or claim that a proposed patch is accepted.
