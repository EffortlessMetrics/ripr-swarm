# Agent quickstart

Wire `ripr` into a coding agent in one step. `ripr` is a static test-gap finder:
it reads your diff and names the changed behavior no current test would notice.
It never runs tests or mutants, never edits files, and its answers are static
evidence, not proof that tests catch real bugs.

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
5. DONE    when `ripr check --quiet --json | jq '[.findings[]|select(.classification!="exposed")]|length'`
           prints 0 (human form: "State: no gap selected for repair").
           Report what you added; ripr's pass is static, not proof that tests catch real bugs.

STOP and report instead of looping when:
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

The block carries the rule, restated here because it is the part agents get
wrong: **done** is `ripr check --quiet` reporting no gap selected for repair, and
**stop** is an analyzer limit, three exact-assertion tests that leave the same
gap (often an equivalent change such as `>` versus `>=` where both branches
return the same value), or exit code 2 or 3. Stop means report to the human; it
does not mean change production code until the gap disappears.

## More

- [Quickstart: agent repair details](QUICKSTART.md#agent-repair-details)
- [Exit codes](EXIT_CODES.md)
- [LLM operator guide](LLM_OPERATOR_GUIDE.md) (the artifact-level workflow; large)
- `AGENTS.md` at the repository root is for people developing `ripr` itself, not
  for projects that use it.
