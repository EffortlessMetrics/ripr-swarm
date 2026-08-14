# Immutable Git-tree candidate analysis

Issue: #3237  
Downstream consumer: `EffortlessMetrics/perl-lsp-swarm#9112` / terminal cleanup #9119  
Branch: `agent/immutable-git-tree-candidate`  
Base at creation: `46acf65c180adf234f820472013cf3a0bd46330d`

## End goal

Give RIPR one producer-native way to analyze an exact immutable Git candidate selected by a caller. The caller supplies a base identity and a frozen candidate tree OID; RIPR derives the diff and reads candidate source/tests/config from that bound Git object view without consulting the dirty worktree or a later live-index snapshot. Existing ordinary `--base`, `--diff PATH`, repo analysis, and LSP behavior must remain compatible. This issue owns input authority only; #3212 and #3213 continue to own currentness and test-source-role semantics.

## Codex implementation order

1. Read `crates/ripr/src/app.rs`, `app/check/*`, `analysis/diff/*`, `analysis/pipeline.rs`, `analysis/language/rust.rs`, Git process/timeout helpers, config loading, output identity contracts, and the tests around `CheckInput` before designing a new public shape.
2. Introduce one typed internal/library candidate subject. Avoid scattering `Option<String>` combinations through adapters.
3. Define exact resolution semantics for repository root, base treeish, candidate tree OID, and unborn/empty-tree callers. Invalid or missing objects fail closed as named input/instrument states.
4. Add a Git-object adapter that derives `base -> candidate` diff and provides candidate-tree bytes/filesystem view to the existing analysis pipeline without treating the live index/worktree as authority.
5. Decide the narrowest API/CLI spelling that preserves current contracts. Do not overload `--diff PATH` with hidden tree semantics and do not make a bare `--staged` flag the only API because downstream callers may already hold a frozen tree OID.
6. Make effective config loading candidate-aware: candidate `ripr.toml` and other analysis-governing inputs must come from the candidate subject.
7. Stamp machine-visible repository/base/candidate/diff/config/tool/completeness identity in the output path used by machine consumers.
8. Build the concurrency/subject-leakage falsifier corpus before claiming the API complete.
9. Prove normalized parity between immutable-tree analysis and an equivalent committed checkout of the same tree.
10. Preserve existing process/network/scope guards and run the full output-contract/golden/fixture checks required by the repo.

## Subject contract

Conceptually:

```text
repository/root = selected Git repository
base            = explicit committed/treeish identity
candidate       = explicit immutable tree OID
diff            = producer-derived base -> candidate
source/tests     = candidate tree
ripr.toml        = candidate tree
dirty worktree  = non-authoritative
live index       = non-authoritative after binding
```

CLI spelling may resemble:

```bash
ripr check --root . --base-tree <BASE> --candidate-tree <TREE_OID> --mode draft --format json
```

but the implementation should choose names consistent with RIPR's existing CLI/output vocabulary.

## Architecture expectations

Prefer a typed seam such as:

```text
CheckSubject
  ExistingDiff / BaseHead / Repo / ImmutableGitCandidate
```

or an equivalent design that prevents contradictory base/diff/candidate combinations. Language adapters should receive normalized analysis inputs and not need to know whether the candidate came from a worktree checkout or Git object view unless byte access genuinely requires it.

Reuse the repo's existing bounded Git invocation/timeout/process policy. Do not add a second unbounded Git runner.

## Machine-visible identity

At minimum expose enough typed state for a consumer to bind receipts/cache to:

```text
repository/root identity
base identity
candidate tree OID
diff identity
analysis mode
effective config identity
RIPR version/output schema
complete / limited / not-proven state
```

Avoid volatile temp paths in portable semantic identity.

## Mandatory falsifiers

- candidate source is bad while dirty worktree contains an unstaged repair: result follows candidate;
- candidate test is strong while worktree test is weaker: candidate test wins;
- candidate `ripr.toml` differs from worktree config: candidate config wins;
- index changes after subject binding: result remains unchanged;
- worktree changes after subject binding: result remains unchanged;
- added/modified/deleted/renamed/type-changed Rust paths;
- whole-file delete succeeds without requiring worktree file presence;
- invalid/missing candidate tree fails closed;
- candidate from another repository/root cannot be silently accepted;
- same candidate through immutable-tree mode and equivalent committed checkout normalizes identically;
- Windows and POSIX paths are deterministic;
- current `--diff PATH` tests remain green.

## Interaction with #3212 and #3213

Do not hide semantic fixes inside this PR merely to make downstream examples pretty.

- #3212 owns whether base-deleted/moved probes can become candidate-actionable.
- #3213 owns whether test/evidence plumbing creates production proof obligations.

This PR may carry the source/currentness metadata needed for those issues to make decisions, but acceptance of this issue is specifically about **immutable input authority**.

## Performance

Record Git/object/materialization cost separately from analyzer cost. Preserve existing diff scope/file/line guards and partial/limited honesty. The subject should be naturally cacheable by candidate tree + base + config/tool identity; do not introduce a mutable cache key tied to current index state.

## Likely touch points

```text
crates/ripr/src/app.rs
crates/ripr/src/app/check/*
crates/ripr/src/analysis/diff/*
crates/ripr/src/analysis/pipeline.rs
candidate-aware workspace/config byte access helper(s)
CLI parse/help for the new explicit subject
output identity / schemas where required
focused Git fixture/falsifier tests
spec/traceability/output-contract docs where public contract changes
this plan file
```

## Guardrails

- No hook installation.
- No perl-lsp-specific suppression policy.
- No automatic mutation execution.
- No claim this replaces remote integration proof.
- No requirement to repin frozen 0.11.0.
- No breaking change to ordinary `--diff PATH` without an explicit compatibility decision.

## Acceptance before merge

- one typed immutable candidate subject exists;
- candidate source/test/config bytes cannot come from dirty worktree or later index reads;
- base/candidate/diff/config identities are machine-visible;
- concurrent mutation falsifiers pass;
- committed-equivalent parity passes;
- invalid subjects fail closed;
- existing diff/repo/LSP contracts remain green;
- #3212/#3213 remain separately owned.

## Suggested review map

Review typed subject/API shape first, Git-object authority second, candidate-aware byte/config access third, identity/output changes fourth, falsifier corpus last. Any use of `git diff --cached`, live-index reads, or ordinary filesystem reads after candidate binding should be treated as suspect unless proven non-authoritative.
