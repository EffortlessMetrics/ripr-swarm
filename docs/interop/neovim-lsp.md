# Neovim standard-LSP proof recipe

This recipe is a **candidate proof surface for [#1630](https://github.com/EffortlessMetrics/ripr-swarm/issues/1630)**.
It does not promote Neovim, generic editors, or any Neovim version range to a supported tier.
The support claim is earned only after the journey below is run against an unmodified `ripr` binary and its receipt is reviewed.

The point of the exercise is narrow: prove that ripr's portable LSP baseline works through an off-the-shelf client,
without VS Code extension methods, client-specific shims, or ripr source edits.

## Boundary under test

| Surface | Contract used here |
|---|---|
| Client | Neovim's built-in `vim.lsp` client |
| Server command | `ripr lsp --stdio` |
| Language | Rust |
| Root selection | `ripr.toml`, then `.git`, then `Cargo.toml` |
| Initialization | Standard LSP initialize request; no ripr-specific initialization options |
| Configuration | Standard `workspace/configuration` handling |
| Refresh | Server-owned `ripr.refresh` through `workspace/executeCommand` |
| Source authority | Saved workspace on disk |
| Enhanced actions | Not expected; `riprEditor` and `riprAgent` capabilities are intentionally absent |

Neovim should start one ripr client per resolved root. That makes the root used in `initialize` explicit and avoids
silently folding independent repositories into one server session.

This recipe uses `vim.lsp.config()` / `vim.lsp.enable()` era APIs. Run it only on a Neovim build that provides those
APIs, and record the exact version in the receipt. A successful run establishes evidence for that version; it does
not establish a version range.

## Configure the client

Create `lsp/ripr.lua` under Neovim's configuration directory. From Neovim, this opens the exact path:

```vim
:execute 'edit' stdpath('config') .. '/lsp/ripr.lua'
```

Use this configuration:

```lua
return {
  cmd = { "ripr", "lsp", "--stdio" },
  filetypes = { "rust" },
  root_markers = { "ripr.toml", ".git", "Cargo.toml" },
  workspace_required = true,
}
```

Save the config with `:write ++p` so Neovim creates the `lsp/` directory when needed.

Enable it from `init.lua`:

```lua
vim.lsp.enable("ripr")
```

Do not add `init_options`, custom handlers, command rewrites, or VS Code compatibility methods for the baseline run.
A client-side workaround may be useful later, but it would invalidate this proof.

## Preconditions

Run these from the same environment that launches Neovim:

```text
ripr --version
ripr lsp --version
```

Both commands must resolve the same installed binary that Neovim will find on `PATH`. Confirm Neovim's resolved
path before starting the server:

```vim
:lua print(vim.fn.exepath("ripr"))
```

Choose a Rust repository that:

- has at least one known ripr finding with diagnostic, hover, code-action, and code-lens evidence;
- can be modified and restored safely;
- has a stable root marker;
- is not already open in another Neovim process running a different ripr build.

This recipe intentionally covers Rust only. Preview-language adapters need their own client proof because filetype,
root, and capability behavior can differ.

## Run the client journey

Record every result as `pass`, `fail`, `limited`, or `not_run`. Preserve the first failure and the relevant log
excerpt rather than smoothing the run into a single pass/fail claim.

### 1. Initialize and attach

Open a Rust source file below the chosen root, then run:

```vim
:checkhealth vim.lsp
:lua print(vim.inspect(vim.lsp.get_clients({ name = "ripr", bufnr = 0 })))
```

Confirm:

- exactly one ripr client is attached to the buffer;
- `client.config.cmd` is `ripr lsp --stdio`;
- `client.root_dir` is the intended repository root;
- the client reached `initialized = true`;
- no extension-private method was required during startup.

Capture the negotiated position encoding and the server capabilities from the client object.

### 2. Observe diagnostics

Open a file and location with a known ripr finding. Inspect both presentation and retained diagnostic data:

```vim
:lua print(vim.inspect(vim.diagnostic.get(0)))
```

Confirm that the expected diagnostic appears at the correct range and severity. Inspect
`user_data.lsp.relatedInformation` when present and record whether Neovim retained the related evidence even when it
did not render every field in the default UI.

This distinction matters: missing presentation is a client-adapter limitation; missing protocol evidence is a server
or negotiation failure.

### 3. Exercise hover, code actions, and code lenses

Place the cursor on the known seam and run:

```vim
:lua vim.lsp.buf.hover()
:lua vim.lsp.buf.code_action()
:lua vim.lsp.codelens.enable(true, { bufnr = 0 })
:lua print(vim.inspect(vim.lsp.codelens.get({ bufnr = 0 })))
```

Confirm:

- hover returns the expected ripr explanation;
- at least the expected standard code action is visible;
- code lenses can be requested and inspected;
- any action that is absent because enhanced client capabilities were not advertised is recorded as `limited`, not
  silently treated as a server failure.

Run a lens only when its effect is understood and reversible:

```vim
:lua vim.lsp.codelens.run()
```

### 4. Prove saved-workspace refresh

Make a small change that should alter the known finding, but do not save it yet.

Record what changes, if anything, while the buffer is dirty. This observation is not a dirty-buffer correctness
claim: ripr's current portable contract is saved-workspace truth, and [#1625](https://github.com/EffortlessMetrics/ripr-swarm/issues/1625)
owns the remaining synchronization boundary.

Save the file:

```vim
:write
```

Then invoke the server's advertised standard command through the attached client:

```vim
:lua local c = assert(vim.lsp.get_clients({ name = "ripr", bufnr = 0 })[1], "ripr is not attached"); c:exec_cmd({ title = "Refresh RIPR", command = "ripr.refresh" }, { bufnr = 0 })
```

Confirm that diagnostics, hover, and code lenses converge on the saved file without restarting Neovim or editing
ripr source.

Restore the file, save again, refresh again, and confirm that the original evidence returns.

### 5. Observe progress and logs

While refresh or analysis is running:

```vim
:lua print(vim.lsp.status())
:lua print(vim.lsp.log.get_filename())
```

For a diagnostic run, enable debug logging before starting the client:

```vim
:lua vim.lsp.log.set_level("debug")
```

After collecting the evidence, restore normal logging:

```vim
:lua vim.lsp.log.set_level("warn")
```

Record whether progress was visible and whether the log contains clean initialize, request, refresh, shutdown, and
exit sequences. Redact local paths or other sensitive values before attaching excerpts to a public issue or PR.

### 6. Prove root isolation

Open Rust files from two independent repositories in the same Neovim process. Inspect the clients:

```vim
:lua print(vim.inspect(vim.lsp.get_clients({ name = "ripr" })))
```

Confirm:

- each repository has a client whose `root_dir` matches that repository;
- buffers do not attach to the other repository's client;
- diagnostics and refresh stay within the selected root;
- the result does not depend on changing Neovim's process-wide working directory.

If ripr later supports one multi-folder server session as a deliberate contract, prove that separately. This recipe
tests the simpler and less ambiguous one-client-per-root baseline.

### 7. Shut down cleanly

Stop the attached ripr client through Neovim:

```vim
:lsp stop ripr
```

Alternatively, exercise the programmatic path and allow up to one second before force-stop:

```vim
:lua for _, c in ipairs(vim.lsp.get_clients({ name = "ripr" })) do c:stop(1000) end
```

Confirm that the server receives shutdown/exit, the client disappears from `vim.lsp.get_clients()`, and no orphaned
`ripr lsp --stdio` process remains. Record the platform-specific process check used.

## Receipt

Store the result as JSON using this shape. It is a proof record for #1630, not a stable ripr output schema.

```json
{
  "schema": "ripr-neovim-standard-lsp-proof/v1",
  "captured_at": "YYYY-MM-DDTHH:MM:SSZ",
  "client": {
    "name": "Neovim",
    "version": "exact output of nvim --version",
    "configuration": "built-in vim.lsp; no LSP plugin"
  },
  "server": {
    "ripr_version": "exact output of ripr --version",
    "lsp_version": "exact output of ripr lsp --version",
    "binary": "resolved executable path",
    "command": ["ripr", "lsp", "--stdio"],
    "ripr_source_modified": false
  },
  "platform": {
    "os": "name and version",
    "arch": "architecture",
    "shell": "launch environment"
  },
  "workspace": {
    "language": "rust",
    "fixture_or_repository": "public identifier or redacted local description",
    "root_marker": "ripr.toml|.git|Cargo.toml",
    "resolved_root": "path or redacted stable token",
    "second_root": "path, redacted stable token, or null"
  },
  "negotiated": {
    "position_encoding": "utf-8|utf-16|utf-32",
    "server_capabilities": {},
    "extension_private_methods_used": []
  },
  "journey": {
    "initialize": {
      "state": "pass|fail|limited|not_run",
      "evidence": ""
    },
    "diagnostics": {
      "state": "pass|fail|limited|not_run",
      "evidence": "",
      "related_information_retained": null
    },
    "hover": {
      "state": "pass|fail|limited|not_run",
      "evidence": ""
    },
    "code_actions": {
      "state": "pass|fail|limited|not_run",
      "evidence": ""
    },
    "code_lens": {
      "state": "pass|fail|limited|not_run",
      "evidence": ""
    },
    "dirty_buffer_observation": {
      "state": "pass|fail|limited|not_run",
      "evidence": "",
      "authoritative": false
    },
    "save_refresh": {
      "state": "pass|fail|limited|not_run",
      "evidence": ""
    },
    "progress": {
      "state": "pass|fail|limited|not_run",
      "evidence": ""
    },
    "root_isolation": {
      "state": "pass|fail|limited|not_run",
      "evidence": ""
    },
    "shutdown": {
      "state": "pass|fail|limited|not_run",
      "evidence": "",
      "orphan_processes": null
    }
  },
  "canonical_observation": {
    "source_authority": "saved_workspace",
    "first_failure": null,
    "known_limits": [
      "No Neovim version range is claimed by this receipt.",
      "Dirty buffers are observational until the #1625 synchronization contract is earned.",
      "VS Code-enhanced actions are outside the standard-client baseline."
    ]
  },
  "normalized_sequence": [
    "initialize",
    "initialized",
    "textDocument/didOpen",
    "diagnostic publication or pull",
    "textDocument/hover",
    "textDocument/codeAction",
    "textDocument/codeLens",
    "textDocument/didChange",
    "textDocument/didSave",
    "workspace/executeCommand:ripr.refresh",
    "shutdown",
    "exit"
  ],
  "measured": {
    "client_count_first_root": null,
    "client_count_two_roots": null,
    "diagnostic_count_initial": null,
    "diagnostic_count_dirty_observation": null,
    "diagnostic_count_after_saved_refresh": null,
    "diagnostic_count_after_restore_refresh": null,
    "refresh_duration_ms": null
  }
}
```

Replace placeholders with measured values. Do not report zero counts merely because the client UI hid a field; derive
them from the retained LSP data or mark them unknown.

## Acceptance boundary

A reviewed receipt may establish that a specific Neovim build can complete ripr's standard LSP journey on the tested
platform and fixture. It does not by itself establish:

- dirty-buffer authority;
- generic support for all Neovim versions or platforms;
- parity with the VS Code extension;
- support for preview-language adapters;
- compatibility with LSP proxy plugins, headless agent clients, or MCP;
- mutation execution through the editor.

Those are separate claims with separate evidence.
