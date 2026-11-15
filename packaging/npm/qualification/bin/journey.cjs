#!/usr/bin/env node
"use strict";

// Usage: node journey.cjs --packages <dir> --target <rust-target> --version <x.y.z>
//   --receipt <file> [--log <file>] [--source-commit <sha>] [--source-tree <sha>]
//   [--expected-version-line "ripr x.y.z (<sha>)"] [--work <dir>] [--keep]
const { main } = require("../lib/journey.cjs");

main(process.argv.slice(2)).then(
  (code) => { process.exitCode = code; },
  (error) => {
    process.stderr.write(`ripr npm journey: ${error && error.message ? error.message : error}\n`);
    process.exitCode = 2;
  },
);
