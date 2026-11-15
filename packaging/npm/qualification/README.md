# npm install-to-use journey

Proves a clean consumer can install `@effortlessmetrics/ripr` from staged
tarballs through npm's real optional-dependency resolution and use it on a
TypeScript project, without a Rust toolchain, a source checkout, a runtime
download or an ambient `ripr` (#4627). The release rehearsal runs it per
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
4. Checks that the installed launcher's own resolver selects bytes whose
   SHA-256 equals the executable inside the staged native tarball.
5. Runs `npm exec --package=@effortlessmetrics/ripr@<version>` from a
   directory with no project, then tears the registry down and checks that
   its port refuses connections.
6. With no registry, runs everything offline: `npm run ripr:version` and
   `npm run ripr:check` (`ripr check --format json` must report a finding on
   the changed TypeScript file and an explain continuation), the continuation
   through `npm exec --prefix` from a foreign CWD, and the public
   `node_modules/.bin/ripr` (POSIX).
7. Controls: an `--omit=optional` install must fail with the exact package
   name, version and reinstall advice. A tampered native version must be
   refused. `npm ci` from the lockfile must restore identical bytes. `npm
   uninstall` must remove the launcher, the payload and the bin shim. The
   decoy must never run.

The receipt is a typed `package_qualification_receipt`. Any failed step fails
the row, and a row without an executed payload hash can't pass. The log keeps
bounded stdout and stderr tails for each command.

Limits: npm is the only package manager exercised. Upgrading between two
released versions isn't exercised. Nothing here publishes, signs or uses
registry tokens.
