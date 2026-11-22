#!/usr/bin/env node
"use strict";

// Usage: node journey.cjs --packages <dir> --target <rust-target> --version <x.y.z>
//   --receipt <file> [--log <file>] [--source-commit <sha>] [--source-tree <sha>]
//   [--expected-version-line "ripr x.y.z (<sha>)"] [--work <dir>] [--keep]
const { main } = require("../lib/journey.cjs");

// Do not call process.exit() on the way out. On aarch64-apple-darwin it has
// deadlocked after a fully passed journey: the main thread sat in
// NodePlatform::Shutdown joining a V8 baseline-compiler thread that was itself
// waiting for the main thread to run a garbage collection. Setting the exit
// code and letting the loop drain takes Node's ordinary teardown, which stops
// those jobs first. The unreferenced timer only fires if a stray handle is
// holding the loop open, which is the case the explicit exit was there for.
function finish(code) {
  process.exitCode = code;
  setTimeout(() => process.exit(code), 10000).unref();
}

main(process.argv.slice(2)).then(
  finish,
  (error) => {
    process.stderr.write(`ripr npm journey: ${error && error.message ? error.message : error}\n`);
    finish(2);
  },
);
