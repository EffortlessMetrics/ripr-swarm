"use strict";

const path = require("node:path");
const { spawnSync } = require("node:child_process");

const REQUIRED_PORTABLE_TESTS = Object.freeze([
  "rejects lifecycle scripts, version ranges, and dependency drift",
  "rejects missing, wrong-version, wrong-target, traversal, symlink, directory, and non-executable payloads",
  "source bin missing-package failure never falls back to PATH or writes stdout",
  "npm package contents are explicit and exclude tests and build residue",
]);
const REQUIRED_POSIX_TESTS = Object.freeze([
  "forwards direct SIGTERM to native child exactly once and re-emits signal",
  "observes terminal SIGINT without forwarding a duplicate to the native child",
]);
const requiredTests = process.platform === "win32"
  ? REQUIRED_PORTABLE_TESTS
  : [...REQUIRED_PORTABLE_TESTS, ...REQUIRED_POSIX_TESTS];

const result = spawnSync(
  process.execPath,
  ["--test", "--test-reporter=tap", path.join(__dirname, "launcher.test.cjs")],
  {
    cwd: path.join(__dirname, ".."),
    env: process.env,
    encoding: "utf8",
  },
);

if (result.stdout) {
  process.stdout.write(result.stdout);
}
if (result.stderr) {
  process.stderr.write(result.stderr);
}
if (result.error) {
  throw result.error;
}
if (result.status !== 0) {
  process.exitCode = typeof result.status === "number" ? result.status : 1;
} else {
  const passed = new Set();
  for (const line of result.stdout.split(/\r?\n/)) {
    const match = line.match(/^\s*ok \d+ - (.+)$/);
    if (!match) {
      continue;
    }
    const directive = match[1].match(/^(.*?)(?: # (SKIP|TODO)\b.*)?$/);
    if (directive && !directive[2]) {
      passed.add(directive[1]);
    }
  }
  const missing = requiredTests.filter((name) => !passed.has(name));
  if (missing.length > 0) {
    process.stderr.write(
      `required launcher controls did not execute and pass: ${missing.join(", ")}\n`,
    );
    process.exitCode = 1;
  } else {
    process.stdout.write(`verified ${requiredTests.length} required launcher controls\n`);
  }
}
