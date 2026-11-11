# Agent quickstart

Wire `ripr` into a coding agent in one step. `ripr` is a static test-gap finder:
it reads your diff and names the changed behavior no current test would notice.
It never runs tests or mutants, never edits files, and its answers are static
evidence, not proof that tests catch real bugs.

For Muse, follow the [native plugin guide](../plugins/muse/ripr/README.md)
for installation, MCP approval, and the `ripr-setup` and `ripr-repair` skills.
The commands below generate instruction files for Claude Code, Codex, and Cursor.

```bash
ripr init --agents claude    # CLAUDE.md             (Claude Code)
ripr init --agents codex     # AGENTS.md             (Codex and other AGENTS.md readers)
ripr init --agents cursor    # .cursor/rules/ripr.mdc (Cursor)
ripr init --agents all       # all three
```

Add `--dry-run` first to see exactly what would be written. Then commit the file
so every session sees it.

## Python and TypeScript repos

`--agents` looks at the repo root for `Cargo.toml`, `pyproject.toml` / `setup.py`
/ `pytest.ini`, and `package.json` / `tsconfig.json`. A Rust repo gets the Rust
block below. A Python or TypeScript/JavaScript repo gets a variant with the
test command (`pytest` or `unittest`, `jest` or `vitest`) and assertion advice
that fit, and no promise of guided `ripr repair`, which covers Rust only; the
route is `ripr check --format agent`, then a hand-written test. A repo with
Rust plus another language keeps the Rust block and adds a short note. The
Cursor rule's `globs` and description follow the detected languages. Detection
looks at the root and its immediate children only. A detected language counts
only when `[languages].enabled` in `ripr.toml` turns it on; otherwise the block
says those files are not analyzed and points at `ripr doctor` instead of
promising a loop. `--force` also refreshes the Cursor front matter, but only
when it is exactly the one ripr wrote; your own front matter is kept.

Every language variant uses the same compact post-edit `check --format agent`
report for VERIFY and DONE. The final `verify=` record applies the full-count,
scope and source-currentness checks; preview results remain advisory. No
variant requires a full JSON report for ordinary completion.

## What `--agents` does to your files

- It writes a short block (about 700 tokens) between `<!-- ripr:agents:begin ... -->`
  and `<!-- ripr:agents:end -->` markers.
- A missing file is created. An existing file keeps all of your content; the
  section is appended after it.
- A rerun changes nothing. If you edited text inside the markers, a plain rerun
  keeps your edit; `--force` replaces only the marked section with the current
  text.
- A symlinked file, a file that is not valid UTF-8 text, or a start marker with
  no end marker is refused and nothing is written.
- `ripr.toml` is created only if it is missing, as with `--mcp`; an existing one
  is kept.

## The block

This is the exact text `ripr init --agents` writes. It is also the whole
loop, so you can paste it by hand into any agent instruction file.

```markdown
## ripr: static test-gap finder (Rust). Run it after you change behavior.
ripr reads your diff and says which changed lines no test would notice. It never runs
tests or mutants; "exposed" is static evidence only. It does not edit files.

Before you start: commit the source change; `/target/` must be in .gitignore.
Only edit tests. Never change production code to make a gap go away.
Keep the same --base and source options for every check below.

Loop (max 3 rounds per gap):
1. FIND    `ripr check --quiet --format agent` (add `--base <branch>` if no origin/main|master).
           Read gap, test and next lines. Exit 0 can still mean gaps; 2/3 = stop (read stderr).
2. DETAIL  Follow the printed `ripr explain ...` with its selector and scope. Add `--format agent`
           only if no `--format` is present; for `--from` plus `--suppression-policy`, keep it as printed.
           Follow `ripr context ...` as-is (JSON). Optional: `ripr agent stub --at FILE:LINE --kind KIND [--write]`.
3. WRITE   One focused test next to the related test. Use assert_eq! on exact changed
           values: every changed field, branch and boundary (below, equal, above). Not
           `.is_some()`, `.is_empty()`, `<=`, or only whole-struct equality.
4. VERIFY  `cargo test <your_test_name>` must pass. Then run `ripr check --quiet --format agent`
           once more on the same root, base and saved source (new files need `git add -N PATH`).
           Check its exit status before reading it; do not change inputs before DONE.
5. DONE    Use that same report; require exit 0 and its last line `verify=complete|...`.
           `verify=refused`, a missing `verify=` line, or a cut-off report mean STOP.
           `done=` or an empty gap list alone is never enough.
           If gaps remain, return to FIND; if inputs change, repeat VERIFY before DONE.
           Report tests and static result; this does not establish runtime bug detection.

STOP and report instead of looping when:
- the outcome names empty scope, no changed behavior, partial analysis, unsupported input or failed analysis;
- the top finding says "Analyzer limit" / "Tests cannot close this static-analysis limitation";
- the same gap remains after 3 exact-assert tests (it may be an equivalent change, e.g. `>` vs
  `>=` where both branches return the same value); or `ripr check` exits 2 or 3.

Optional guided repair (Rust seams ripr can bind to one test file):
  `ripr repair <seam-id-or-probe-id> --root .` -> edit only the named test file -> `ripr continue`
  If it refuses (exit 2 or 3), fall back to steps 3-5. `ripr status --root .` resumes an attempt.

Do not read target/ripr/pilot/*.json (2-4 MB) or docs/OUTPUT_SCHEMA.md (1 MB).
MCP is optional and read-only; the CLI above is cheaper. Setup: ripr init --mcp
```

## Optional: register the MCP server

The MCP server exposes a bounded, read-only evidence workflow. Start with
`ripr_workspace_status`, call `ripr_refresh`, then use `ripr_list_gaps` and
`ripr_get_gap` to inspect one selected item. `ripr_prepare_repair` prepares an
eligible item's in-memory transaction or reports the typed limitation. It does
not edit source or tests, run verification, or create a durable CLI attempt.
See the [progressive MCP flow](interop/mcp.md#progressive-flow) for repair
cards, attempt and receipt inspection, and snapshot-pinned reads.

Refresh explicitly after edits. Like `ripr check`, `ripr_refresh` reads saved
tracked edits when any exist; pass `source: "committed"` or
`source: "worktree"` to force one, and repeat any explicit `base` on each
refresh. The server does not watch the worktree.

The CLI loop above is cheaper. The default MCP tool list alone is about 12 KB
(11,884 bytes on the wire) at every session start. Register it only if your
agent prefers tools. `ripr` must be on
`PATH`.

| Agent | Register |
| --- | --- |
| Claude Code | `ripr init --mcp` writes `.mcp.json` (keeps your other servers), or `claude mcp add --scope project ripr -- ripr mcp --stdio`. Claude Code asks you to approve the project server. |
| Codex | `codex mcp add ripr -- ripr mcp --stdio` (user-global config; `init` never writes it). |
| Cursor | `ripr init --agents cursor --mcp` writes `.cursor/mcp.json` (keeps your other servers), or add the entry below by hand. |

Cursor and Claude Code share one entry shape:

```json
{
  "mcpServers": {
    "ripr": {
      "command": "ripr",
      "args": ["mcp", "--stdio"],
      "env": { "RIPR_MCP_TOOLS_LIST": "slim" }
    }
  }
}
```

The slim `tools/list` is the default, and the `env` line keeps older ripr builds slim too (results are unchanged). If your host validates results against `outputSchema`, set the value to `full`.

`ripr init --mcp` and the Cursor file were exercised against the real
`ripr` binary. The `claude`, `codex` and Cursor clients' own registration and
approval steps were not run here; treat those lines as configuration, not a
support claim. Full MCP details:
[MCP evidence and repair preparation](interop/mcp.md).

## Choosing between `explain` and `context`

For one finding, pick by what the consumer is:

| Need | Command | Output |
|---|---|---|
| A model reading a few lines under a token budget | `ripr explain <id> --format agent` | compact lines with an explicit `done=` / `open=` / `stop=` terminal line and a `next=` context command |
| A person reading why it was flagged | `ripr explain <id>` | human explanation |
| A tool or agent that needs fields (related tests, seams, evidence) | `ripr context --at <id>` | JSON packet (always JSON) |

`explain --format agent` is a static advisory projection, not a runtime result.
`explain --format json` is refused; use `ripr context`.

## When the agent should stop

**Done** requires the focused test to pass and the successful post-edit
`ripr check --quiet --format agent` for the same root, base and saved source
used by the loop. Use that single report for both verification and the done
decision; if inputs change, run the check again. Read its last line, not just
`done=`, an empty gap list or the human message `State: no gap selected for
repair`. That `verify=` record is projected from the producer's outcome and
full summary counts, never from the displayed subset:

- `verify=complete|outcome=O|findings=N|exposed=N` means the outcome is
  `complete_no_findings` or `complete_with_findings`, the full
  `findings` and `exposed` counts are equal, and no unanalyzed working tree,
  untracked source in an enabled language excluded from the diff, partial or
  no-scope condition applies. A new file stays untracked until
  `git add -N <path>`, so stage new files before the post-edit check.
  Gitignored source is named in the notes but does not refuse.
- `verify=refused|...|reason=R,...` names every unmet condition
  (`outcome_missing`, `no_behavior_in_scope`, `analysis_incomplete`,
  `gaps_remain`, `unanalyzed_working_tree`, `untracked_source_excluded`,
  `partial_scope`, `no_scope`).
  `no_scope`, `no_changed_lines` and `no_behavioral_candidates` are complete
  analyses of an empty scope; they do not establish completion of the
  requested behavior review.
- The record is the last line, after the bounded gap lines, so a truncated
  report cannot leave a leading success marker. A missing `verify=` line means
  STOP.

The full JSON (`--format json`) remains an explicit drilldown, not ordinary
model input. Test execution stays a separate proof (`cargo test`).

**Stop and report** the actual state when the outcome is empty, partial,
unsupported or failed, required evidence is missing, or output is truncated.
For an empty scope, confirm the intended base and saved source, then rerun
with the appropriate `--base` or `--worktree` option; do not interpret zero
findings as an all-clear. For truncation, follow the report's output-budget
recovery and inspect a complete report. Check the command's own exit status
before reading its JSON so a successful downstream filter cannot hide a failed
check.

An analyzer limit, three exact-assertion tests that leave the same gap (often
an equivalent change such as `>` versus `>=` where both branches return the
same value), or exit code 2 or 3 also means stop and report. It does not mean
change production code until the gap disappears. A completed static loop
still does not establish that tests catch real bugs.

## More

- [Quickstart: agent repair details](QUICKSTART.md#agent-repair-details)
- [Exit codes](EXIT_CODES.md)
- [LLM operator guide](LLM_OPERATOR_GUIDE.md) (the artifact-level workflow; large)
- `AGENTS.md` at the repository root is for people developing `ripr` itself, not
  for projects that use it.
