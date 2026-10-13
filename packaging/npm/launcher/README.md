# ripr

This package is the npm distribution adapter for the native **ripr** CLI.
It is not the VS Code extension and does not implement a JavaScript analyzer.

> Status: source package under development. No npm release is claimed by this
> repository state.

The intended installed command is:

```bash
npm install --save-dev --save-exact @effortlessmetrics/ripr
npm exec -- ripr check --base origin/main
```

The launcher selects one exact-version, platform-specific package and starts its
native executable. It does not run lifecycle installation scripts, download a
binary from GitHub, compile Rust, invoke a shell, or fall back to an unrelated
`ripr` on `PATH`.

When native package resolution fails, the launcher names the exact version,
retains the underlying reason, and advises reinstalling with optional dependencies
enabled. This also covers corrupt metadata, missing manifests, and broken exports
targets: a missing module alone does not establish that the package directory is
absent. Resolution errors go to stderr and exit with status 1.

If direct resolution of the packaged `lib/launcher.cjs` raises Node's
`MODULE_NOT_FOUND`, the command retains that reason and advises restoring the file
from the matching package tarball or reinstalling the same exact launcher version
in its owning npm environment. This advice names the canonical, usable exact
version from readable package metadata. When this recovery branch is reached
without usable metadata, the command tells you to use the exact version selected
for that installation.
It never guesses a newer version or uses the current directory's package.
Other resolution errors and errors inside an existing library retain their own
reason without this restoration hint.

Supported platform packages are currently planned for Linux glibc x64/ARM64,
macOS x64/ARM64, and Windows x64 MSVC. Public support begins only after the
corresponding native payload and installed-product qualification is accepted.

The product is licensed under MIT OR Apache-2.0.
