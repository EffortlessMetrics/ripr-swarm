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
1. FIND    `ripr check --quiet`   (add `--base <branch>` if there is no origin/main|master)
           Read "Start here": File, Missing check, Related test, Next step.
           Exit code is 0 even with gaps. 2 = could not run; 3 = ripr declined (read stderr).
2. DETAIL  Run the `ripr explain ...` / `ripr context ...` lines it printed, exactly as printed
           (ids belong to that run). Optional: `ripr agent stub --at FILE:LINE --kind KIND [--write]`
           prints a compiling skeleton; it fails on purpose (`todo!`) until you fill it in.
3. WRITE   One focused test next to the existing tests (the file "Related test" names).
           Use assert_eq! on the exact changed value(s): every changed field, branch and
           boundary (below, equal, above). Not `.is_some()`, `.is_empty()`, `<=`, or only
           whole-struct equality.
4. VERIFY  `cargo test <your_test_name>` must pass. Then `ripr check --quiet` again
           (a dirty tree is analyzed automatically; new files need `git add -N PATH`).
5. DONE    Read `ripr check --quiet --json` separately; require exit 0 and these fields:
           `analysis_outcome.outcome.kind`: `complete_no_findings` or `complete_with_findings`;
           `analysis_outcome.analysis_complete`: true; `summary.findings` = `summary.exposed`.
           Missing required fields, `analysis_outcome.document_complete: false`, or
           `unanalyzed_working_tree: true` mean STOP. Empty `.findings` alone is never enough.
           Report tests added and the static result; this does not establish runtime bug detection.

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

The CLI loop above is cheaper. The MCP server is read-only workspace status; its
tool list alone costs roughly 11k tokens. Register it only if your agent prefers
tools. `ripr` must be on `PATH`.

| Agent | Register |
| --- | --- |
| Claude Code | `ripr init --mcp` writes `.mcp.json` (keeps your other servers), or `claude mcp add --scope project ripr -- ripr mcp --stdio`. Claude Code asks you to approve the project server. |
| Codex | `codex mcp add ripr -- ripr mcp --stdio` (user-global config; `init` never writes it). |
| Cursor | `ripr init --agents cursor --mcp` writes `.cursor/mcp.json` (keeps your other servers), or add the entry below by hand. |

Cursor and Claude Code share one entry shape:

```json
{
  "mcpServers": {
    "ripr": { "command": "ripr", "args": ["mcp", "--stdio"] }
  }
}
```

`ripr init --mcp` and the Cursor file were exercised against the real
`ripr` binary. The `claude`, `codex` and Cursor clients' own registration and
approval steps were not run here; treat those lines as configuration, not a
support claim. Full MCP details: [MCP workspace status](interop/mcp.md).

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

**Done** requires the focused test to pass and a separate successful
`ripr check --quiet --json` for the same base and source used by the loop.
Read the complete report, not just `.findings` or the human message
`State: no gap selected for repair`:

- `analysis_outcome.outcome.kind` must be `complete_no_findings` or
  `complete_with_findings`, and `analysis_outcome.analysis_complete` must be true.
  `no_scope`, `no_changed_lines` and `no_behavioral_candidates` can also have
  `analysis_complete: true`; they describe an empty analysis scope and do not
  establish completion of the requested behavior review.
- The required numeric `summary.findings` and `summary.exposed` counts must
  match. These full-analysis counts also prevent a rendered subset from hiding
  a remaining gap or a policy-suppressed finding.
- `analysis_outcome.document_complete: false` means the output was truncated;
  `unanalyzed_working_tree: true` means saved changes were not analyzed. Either
  blocks done. These two fields are optional when the condition is absent;
  missing outcome or summary fields do not establish completion.

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
