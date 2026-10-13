#!/usr/bin/env node
"use strict";

const path = require("node:path");
const fs = require("node:fs");

function messageOf(error) {
  return error && typeof error.message === "string" ? error.message : String(error);
}

function recoveryIdentity() {
  // Display context only: the library remains the package-contract validator.
  try {
    const metadata = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "package.json"), "utf8"));
    if (metadata && metadata.name === "@effortlessmetrics/ripr" &&
        typeof metadata.version === "string" &&
        /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(metadata.version)) {
      return `@effortlessmetrics/ripr@${metadata.version}`;
    }
  } catch {
    // Incomplete metadata must not replace the original resolution reason.
  }
  return "the exact @effortlessmetrics/ripr version selected for this npm installation";
}

async function main() {
  let libraryPath;
  try {
    libraryPath = require.resolve("../lib/launcher.cjs");
  } catch (error) {
    if (!error || error.code !== "MODULE_NOT_FOUND") throw error;
    throw new Error(
      `cannot resolve packaged launcher library lib/launcher.cjs: ${messageOf(error)}; restore lib/launcher.cjs from the matching package tarball, or reinstall ${recoveryIdentity()} in its owning npm environment`,
      { cause: error },
    );
  }
  // Import/parser failures inside an existing library retain their own reason.
  const { launch } = require(libraryPath);
  await launch({
    launcherRoot: path.resolve(__dirname, ".."),
    argv: process.argv.slice(2),
  });
}

main().catch((error) => {
  process.stderr.write(`ripr npm launcher: ${messageOf(error)}\n`);
  process.exitCode = 1;
});
