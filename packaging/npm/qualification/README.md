# npm install-to-use journey

Proves a clean consumer can install `@effortlessmetrics/ripr` from staged
tarballs through npm's real optional-dependency resolution and use it on a
TypeScript project. The consumer gets no Rust toolchain on its PATH, no source
checkout and no registry beyond the loopback one, every step after the install
runs with npm offline, and the payload must byte-match the staged tarball. No
ambient `ripr` can stand in (#4627). The release rehearsal runs it per
target against the qualified release tarballs (#4632).

```bash
node packaging/npm/qualification/bin/journey.cjs \
  --packages <dir with effortlessmetrics-ripr-*.tgz> \
  --target x86_64-unknown-linux-gnu --version 0.11.0 \
  --receipt receipt.json --log journey-log.json \
  --source-commit <sha> --source-tree <tree> \
  --expected-version-line "ripr 0.11.0 (<sha>)"
cargo xtask package-qualification-gate --receipt receipt.json \
  --expected-commit <sha> --expected-payload-hash <sha256:...>
```

The receipt binds the identities the journey itself observed. It binds the
release rehearsal only when the gate gets the expected flags:
`--expected-commit`, `--expected-tree`, `--expected-package-hash` (the launcher
tarball) and `--expected-payload-hash` (the qualified payload identity's
executable). `--source-commit` and `--source-tree` are required. A `--work`
directory must be new or empty because the run deletes it afterwards, unless
`--keep` is given, which reports cleanup as `not_run` so the gate refuses the
receipt.

## What one run does

1. Starts `lib/registry.cjs` in its own process: loopback only, ephemeral
   port, read-only, no upstream. It serves only the staged tarballs and
   refuses any tarball that declares an install-time lifecycle script.
2. Builds a consumer `PATH` from the planted decoy `ripr`, Node, git and the OS
   system directories, then checks that `cargo`, `rustc` and `rustup` can't be
   found and that `ripr` resolves only to the decoy.
3. Installs `@effortlessmetrics/ripr@<version>` with `--ignore-scripts` into a
   TypeScript project. Its path contains spaces and non-ASCII characters, it
   has a private npm config and an empty cache, and it holds a committed base
   plus one uncommitted boundary change. The run checks that npm fetched the
   launcher and the target's native tarball, and no foreign native, and that
   the lockfile pins the staged integrity.
4. Checks that the payload the installed launcher's own resolver selects has
   a SHA-256 equal to the executable inside the staged native tarball. The
   receipt's `executed_payload_hash` is this resolved payload. The TypeScript
   check is what shows those bytes behave as ripr: an impostor that prints
   the right version fails there.
5. Runs `npm exec --package=@effortlessmetrics/ripr@<version>` from a
   directory with no project, then tears the registry down and checks that
   its port refuses connections.
6. With no registry, runs everything offline: `npm run ripr:version` and
   `npm run ripr:check` (`ripr check --format json` must report a finding on
   the changed TypeScript file and an explain continuation), the exact
   continuation ripr printed, replayed argument for argument through
   `npm exec --prefix` from a foreign CWD, and the public
   `node_modules/.bin/ripr` (POSIX).
7. Controls: after an `--omit=optional` install, launching must fail with the
   exact package name, version and reinstall advice. A tampered native version
   must be refused, and so must a native whose metadata names another
   target. `npm ci` from the lockfile must restore identical bytes. `npm
   uninstall` must remove the launcher, the payload and the bin shim. The
   decoy must never run.

The receipt is a typed `package_qualification_receipt`. Any failed step fails
the row, and a row without an executed payload hash can't pass. The log keeps
bounded stdout and stderr tails for each command. On POSIX each command runs in
its own process group, so a step that times out takes its descendants down
with it.

Limits: only the recorded Node and npm versions and package manager are
exercised. On Windows the `.bin/ripr.cmd` shim runs only through `npm run` and
`npm exec`. The `ripr` process itself is not network-sandboxed. Upgrading
between two released versions isn't exercised. Nothing here publishes, signs
or uses registry tokens.
