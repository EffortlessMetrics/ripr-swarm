# Changelog

## Unreleased

Extension changes since 0.10.0. Server and CLI changes, which the extension
shows through its diagnostics and commands, are in the
[ripr changelog](https://github.com/EffortlessMetrics/ripr/blob/main/CHANGELOG.md).

### Added

- `ripr: Show Repair Attempt Status` projects the shared CLI repair-attempt
  DTO (`ripr agent status --attempt <id> --json`, RIPR-SPEC-0217) into a
  dedicated status-bar item without re-deriving state: all ten typed status
  classes render at exactly the strength the CLI reports, several current
  attempts require an explicit pick (never a newest/first guess), the
  per-root selection survives restarts, and untrusted workspaces refuse
  before any read ([#4643](https://github.com/EffortlessMetrics/ripr-swarm/issues/4643)).
- `ripr: Start or Continue Repair` runs the shared after phase for one
  resumable live attempt (inventory `disposition: resumable` with an after
  command) and otherwise delegates to the existing start path
  ([#4647](https://github.com/EffortlessMetrics/ripr-swarm/issues/4647),
  advances [#2928](https://github.com/EffortlessMetrics/ripr-swarm/issues/2928)).

- A Get Started walkthrough: trust the workspace, open a Rust file, check
  that the server started (runs `ripr: Diagnose Setup` and lists the fixes
  for a missing server), read ripr diagnostics and try a ripr code action
  ([#2066](https://github.com/EffortlessMetrics/ripr-swarm/issues/2066), [#13638](https://github.com/EffortlessMetrics/ripr-swarm/issues/13638)).
- `ripr: Copy Repair Packet at Cursor` copies the repair packet for the
  diagnostic under the cursor ([#2603](https://github.com/EffortlessMetrics/ripr-swarm/issues/2603)).
- Right-click menu entries for the main ripr actions, and default key
  bindings in a focused editor: `Ctrl+Alt+R` (`Cmd+Alt+R` on macOS) shows
  status, `Ctrl+Alt+P` copies the top repair packet and `Ctrl+Alt+Shift+P`
  copies the packet at the cursor ([#2199](https://github.com/EffortlessMetrics/ripr-swarm/issues/2199), [#2603](https://github.com/EffortlessMetrics/ripr-swarm/issues/2603)).
- When a multi-root window has more than one candidate folder, the extension
  asks which one to analyze instead of picking one ([#2180](https://github.com/EffortlessMetrics/ripr-swarm/issues/2180)).

### Changed

- Command titles all start with a lowercase `ripr:` prefix, and the two
  result-record copy commands have distinct names ([#2052](https://github.com/EffortlessMetrics/ripr-swarm/issues/2052), [#2471](https://github.com/EffortlessMetrics/ripr-swarm/issues/2471)).
- The status bar is colored by state: red for errors, yellow for warnings
  ([#2053](https://github.com/EffortlessMetrics/ripr-swarm/issues/2053)). A limited or deferred run says so instead of showing a
  completed run, and a limited run names the component that degraded, whether
  its findings can be trusted and how to recover ([#4338](https://github.com/EffortlessMetrics/ripr-swarm/issues/4338), [#5046](https://github.com/EffortlessMetrics/ripr-swarm/issues/5046)).
- ripr warnings offer a Show Output action, and an error popup shows the
  server's real failure instead of a generic message ([#2062](https://github.com/EffortlessMetrics/ripr-swarm/issues/2062), [#2167](https://github.com/EffortlessMetrics/ripr-swarm/issues/2167)).
- The managed server download shows progress and says which release it
  fetches ([#2065](https://github.com/EffortlessMetrics/ripr-swarm/issues/2065)). A managed server is installed in a staging folder
  and checked (archive checksum, staged binary, install receipt) before it
  replaces the cached one, and a failed replacement puts the cached one back ([#3184](https://github.com/EffortlessMetrics/ripr-swarm/issues/3184)).
- Changing `ripr.trace.server` applies right away instead of restarting the
  server ([#2168](https://github.com/EffortlessMetrics/ripr-swarm/issues/2168)).
- Agent-loop copy commands run from the Command Palette, without a ripr
  diagnostic to act on, explain how to reach them from a code action instead
  of a generic "not available" message ([#2152](https://github.com/EffortlessMetrics/ripr-swarm/issues/2152)).

### Fixed

- On Windows, the copy actions for agent loop commands accept the server's
  Windows console quoting (`--base "HEAD~1"`, a root path with spaces). They
  were validated against the POSIX single-quote form only and showed "No ripr
  agent loop command is available". A double-quoted span is accepted only on
  Windows, only in the server's exact form, and never with `$`, a backtick,
  `%`, a line break or a typographic quote inside it (#2769).

- Show `$(warning) ripr: language off` and name the language when changed
  files of a language whose adapter is not enabled were skipped, instead of
  the generic `limited` text. The status bar reads the typed
  `unavailable_languages` field of `ripr/analysisStatus`, not message text;
  generic LSP clients get one `window/showMessage` warning per language
  ([#8585](https://github.com/EffortlessMetrics/ripr-swarm/issues/8585)).

- Give the `Extension Smoke` suite setup its own generous mocha budget and
  report a suite whose before-all hook died as a named setup death instead of
  an ordinary test failure. Activation awaits full server start (a 10s
  compatibility probe plus client start) inside the hook, so on a slow or
  cold host the whole real-server configuration/preview e2e suite could die
  in "before all" under the global 10s timeout while the run still looked
  like an ordinary single failure
  ([#6845](https://github.com/EffortlessMetrics/ripr-swarm/issues/6845)).

- Name `ripr.toml [languages] enabled` as the preview-language enablement
  mechanism where expectations are set: the Get Started open-file step and
  the README onboarding show the exact `[languages] enabled = [...]` shape
  plus the restart behavior and scope the step's completion to Rust files,
  and the zero-diagnostics status branch now names routed-but-disabled
  preview languages and the same mechanism instead of a generic
  "enabled languages" hint
  ([#6846](https://github.com/EffortlessMetrics/ripr-swarm/issues/6846)).

- Resolve the diff base from the repository by default. `ripr.baseRef`
  defaulted to `origin/main`, so every repository whose default branch is
  `master` showed "the base `origin/main` does not resolve" and no
  diagnostics until the user found the setting. The default is now empty and
  resolves `origin/HEAD`, then `origin/main`, `origin/master`, `main`,
  `master`, as `ripr check` does. An explicit setting is still used as given.
  When no default branch resolves, the status bar now points at
  `ripr.baseRef` instead of the generic retry text.

- Hover prefers a column-precise finding over a line-level one on the same
  line.

- Start the server when Workspace Trust is granted or a first folder is added.
  Activation in an untrusted or folderless window was recorded as a running
  session, so those events did nothing until `ripr: Restart Server`.
- Name `ripr` commands (`ripr first-pr --root .`, `ripr pr-summary`) in status
  text, notifications and copied guidance instead of `cargo xtask` commands
  that only exist in ripr's own repository. A missing actionable gap queue is
  reported as optional evidence rather than a failure to fix.
- Stop telling users to enable `ripr.server.autoDownload` when it is already
  enabled and the download failed.
- Bind the language server session to the selected folder in multi-root
  windows. The client reported every folder, so the server stayed
  `workspace_ambiguous` after `ripr: Select Workspace Root`. Opening a file
  from one folder now starts the server for that folder, and removing the
  bound folder restarts the session on the remaining root.
- Copy agent-loop commands from real-server code actions when the server binds
  the selected absolute workspace root; reject cross-root, relative, or unsafe
  artifact redirects instead of leaving the previous clipboard text unchanged
  ([#4396](https://github.com/EffortlessMetrics/ripr-swarm/issues/4396)).

- The extension starts in more workspaces: one whose only project marker is
  `tsconfig.json`, `jsconfig.json`, `ripr.toml`, `pyproject.toml`, `setup.py`,
  `setup.cfg`, `requirements.txt`, `pytest.ini`, `tox.ini` or `noxfile.py`
  ([#13615](https://github.com/EffortlessMetrics/ripr-swarm/issues/13615), [#14647](https://github.com/EffortlessMetrics/ripr-swarm/issues/14647), [#14659](https://github.com/EffortlessMetrics/ripr-swarm/issues/14659)).
- A `ripr` on PATH that is older than the extension expects no longer skips
  the managed download. If no managed server can be obtained, the older binary
  is still used, with one warning that names both versions and how to update.
  The `ripr: server missing` status item stays visible after its notification
  is dismissed ([#13638](https://github.com/EffortlessMetrics/ripr-swarm/issues/13638)).
- A managed download that cannot reach its host (offline, DNS, proxy or
  timeout) says so and names the offline route, `ripr.server.path`
  ([#13638](https://github.com/EffortlessMetrics/ripr-swarm/issues/13638)).
- `ripr.server.path` accepts a leading `~` for the home directory, and a
  relative path is refused with a message that asks for the full path
  ([#15075](https://github.com/EffortlessMetrics/ripr-swarm/issues/15075)). On Windows it also accepts a `.cmd` or `.bat` shim such as
  npm's `ripr.cmd` ([#14659](https://github.com/EffortlessMetrics/ripr-swarm/issues/14659)).
- A `ripr.server.downloadBaseUrl` that is not a plain `https://` folder address
  skips the managed download, and the message names the setting and what is
  wrong with the value instead of a bare "Invalid URL" ([#15163](https://github.com/EffortlessMetrics/ripr-swarm/issues/15163)).
- Status messages and setting descriptions name the refresh command by its
  Command Palette title, `ripr: Refresh Full Analysis` ([#15094](https://github.com/EffortlessMetrics/ripr-swarm/issues/15094)). A failed
  refresh points at that command, or at `ripr.baseRef` when the base does not
  resolve ([#4346](https://github.com/EffortlessMetrics/ripr-swarm/issues/4346)).
- A missing server is reported as missing, not as unresponsive ([#5156](https://github.com/EffortlessMetrics/ripr-swarm/issues/5156)),
  and the retry action runs the registered `ripr: Restart Server` command
  ([#2059](https://github.com/EffortlessMetrics/ripr-swarm/issues/2059)).
- A restart racing a start no longer leaves an extra server process running
  ([#2125](https://github.com/EffortlessMetrics/ripr-swarm/issues/2125)).
- `ripr: Show Output` activates the extension when it has not started yet ([#2164](https://github.com/EffortlessMetrics/ripr-swarm/issues/2164)), and
  on Windows the PATH fallback is probed through the shell ([#2166](https://github.com/EffortlessMetrics/ripr-swarm/issues/2166)).
- The Get Started "Trust this workspace" step completes when the workspace is
  already trusted ([#5207](https://github.com/EffortlessMetrics/ripr-swarm/issues/5207)).
- `ripr: Show Repair Route Summary` renders the server's structured summary
  ([#1842](https://github.com/EffortlessMetrics/ripr-swarm/issues/1842)).
- Before use, a resolved server must answer a standard LSP initialize and
  shutdown with the capabilities the extension needs; a server that offers
  unsupported document sync options is refused with the field that failed
  ([#3191](https://github.com/EffortlessMetrics/ripr-swarm/issues/3191), [#4173](https://github.com/EffortlessMetrics/ripr-swarm/issues/4173)).
- Copied agent-loop commands must equal the server's rendering, and
  first-PR, gap-queue and agent-loop redirects must stay inside the workspace ([#4222](https://github.com/EffortlessMetrics/ripr-swarm/issues/4222), [#4236](https://github.com/EffortlessMetrics/ripr-swarm/issues/4236), [#4272](https://github.com/EffortlessMetrics/ripr-swarm/issues/4272)).

### Security

- The server is not downloaded or started until the workspace is trusted, and
  the settings that choose which binary runs are restricted in untrusted
  workspaces ([#1825](https://github.com/EffortlessMetrics/ripr-swarm/issues/1825), [#1867](https://github.com/EffortlessMetrics/ripr-swarm/issues/1867)).
- Before extracting a managed server archive, the extension lists it and
  refuses absolute or `..` member paths, symlinks, hard links, reparse points,
  unsafe Windows names and oversized member counts (over 1000 for tar), for
  both the tar and the Windows zip downloads ([#13656](https://github.com/EffortlessMetrics/ripr-swarm/issues/13656), [#13904](https://github.com/EffortlessMetrics/ripr-swarm/issues/13904)).

## 0.10.0

- Version aligned with ripr 0.10.0. Notes for this release are in the
  [ripr changelog](https://github.com/EffortlessMetrics/ripr/blob/main/CHANGELOG.md).

## 0.9.0

- Version aligned with ripr 0.9.0. Notes for this release are in the
  [ripr changelog](https://github.com/EffortlessMetrics/ripr/blob/main/CHANGELOG.md).

## 0.8.0

- Aligns the marketplace extension metadata with RIPR 0.8.0's evidence-to-repair
  foundation release line.
- Keeps the editor path centered on canonical actionable state, runtime
  completeness, bounded repair packet status, and limitation-route visibility.
- Preserves the preview boundary: no mutation execution, no automatic edits, no
  generated tests, no provider calls, no CI blocking, no gate authority, and no
  public badge semantic switch.

## 0.7.0

- Aligns the marketplace extension metadata with RIPR 0.7.0's actionable
  static repair-loop release line.
- Keeps the editor path centered on saved-workspace status, actionable gap
  queue projection, current repair packets, repo gap maps, and receipt-aware
  refresh guidance.
- Preserves the preview boundary: no mutation execution, no automatic edits, no
  generated tests, no provider calls, no CI blocking, no gate authority, and no
  preview-language promotion.

## 0.6.0

- Aligns the marketplace extension with RIPR 0.6.0's static evidence trust
  and operator-trust release line.
- Keeps the first-run editor path centered on status, saved-workspace
  diagnostics, related-test safety, focused-test briefs, and copied verify or
  receipt commands.
- Surfaces preview language visibility through the server while preserving the
  preview boundary: no mutation execution, no automatic edits, no generated
  tests, no CI blocking, no default gate authority, and no preview-language
  promotion.

## 0.5.0

- Aligns the marketplace extension with RIPR 0.5.0 and the Rust 1.95 server
  baseline.
- Keeps the first-hour editor path centered on saved-workspace diagnostics,
  hovers, intent-titled actions, targeted-test briefs, and copied verify or
  receipt commands.
- Keeps preview limits explicit: no mutation execution, no automatic edits, no
  generated tests, no CI blocking, and no unsaved-buffer overlays by default.

## 0.4.0

- Aligns the marketplace extension with the 0.4 editor-agent evidence loop:
  saved-workspace diagnostics, hover evidence, targeted-test briefs, best
  related-test navigation, and copied agent packet/brief/verify/receipt
  commands now describe the same focused-test workflow.
- Keeps normal editor installs self-provisioned through matching GitHub Release
  server manifests and archives. `cargo install ripr` remains an offline,
  pinned, or controlled-environment fallback.
- Documents that CI users should rely on the generated non-blocking artifact
  workflow, while the CLI remains the shared engine for local proof and
  automation.
- Keeps preview limits explicit: no mutation execution, no automatic edits, no
  CI blocking, no unsaved-buffer overlays by default, and no bundled
  platform-specific VSIXs yet.

## 0.3.1

- Aligns the extension package with the first defaults-first `ripr` release
  line, including saved-workspace seam diagnostics, targeted-test brief actions,
  best related-test navigation, and `draft` mode defaults.
- Keeps server self-provisioning through GitHub Release manifests, with
  `cargo install ripr` as an offline/manual fallback.

## 0.3.0

- Uses the `tower-lsp-server` sidecar from `ripr 0.3.0`.
- Adds diagnostic-targeted context actions so `ripr: Copy Finding Context`
  can use the selected finding location instead of only the active cursor.
- Shows finding-specific hover details for current `ripr` diagnostics.
- Improves diagnostic stability with workspace-root initialization, stale
  diagnostic clearing, refresh failure logging, saved-workspace refresh
  semantics, and serialized refresh generations.

## 0.2.0

- First self-provisioning preview extension.
- Starts `ripr lsp --stdio` from a configured, bundled, downloaded, cached, or
  PATH-discovered server.
- Adds first-run server download and SHA-256 verification from GitHub Release
  manifests.
- Adds commands to restart the server, show the output channel, copy a finding
  context packet, and open settings.
- Keeps `cargo install ripr` as an offline/manual fallback.

## 0.1.0

- Initial PATH-based preview extension scaffold.
