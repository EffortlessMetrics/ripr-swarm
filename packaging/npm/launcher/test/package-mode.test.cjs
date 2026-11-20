"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const crypto = require("node:crypto");
const { spawnSync } = require("node:child_process");
const test = require("node:test");

function mcpReplies(output) {
  assert.equal(output.status, 0, output.stderr);
  assert.equal(output.signal, null);
  const lines = output.stdout.trimEnd().split("\n");
  assert.equal(lines.length, 2, "expected only initialize and tools/list responses");
  const replies = lines.map((line) => JSON.parse(line));
  assert.deepEqual(replies.map((reply) => reply.id), [1, 2]);
  assert.equal(replies[0].result.serverInfo.name, "ripr");
  assert.ok(replies[1].result.tools.length > 0, "tools/list must be nonempty");
  return replies;
}

// Opt in with an absolute path to a built RIPR binary. This stages a local
// test payload; it does not qualify the release producer or a published package.
test("installed npm launcher preserves a real MCP exchange and EOF", (t) => {
  const binary = process.env.RIPR_PACKAGE_TEST_BINARY;
  if (!binary || process.platform !== "linux" || process.arch !== "x64") {
    t.skip("requires RIPR_PACKAGE_TEST_BINARY and Linux x64 glibc");
    return;
  }
  assert.ok(path.isAbsolute(binary), "binary must be an absolute path");
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "ripr-installed-mcp-"));
  try {
    const launcherRoot = path.join(__dirname, "..");
    const version = JSON.parse(fs.readFileSync(path.join(launcherRoot, "package.json"), "utf8")).version;
    const native = path.join(root, "native");
    const project = path.join(root, "project");
    const foreign = path.join(root, "foreign");
    const cache = path.join(root, "npm-cache");
    const npm = process.env.npm_execpath ? process.execPath : "npm";
    const npmPrefix = process.env.npm_execpath ? [process.env.npm_execpath] : [];
    const runNpm = (args, cwd) => {
      const result = spawnSync(npm, [...npmPrefix, ...args], {
        cwd, encoding: "utf8", timeout: 60000,
        env: { ...process.env, npm_config_cache: cache, npm_config_offline: "true" },
      });
      assert.equal(result.status, 0, result.stderr || String(result.error));
      return result.stdout;
    };
    fs.mkdirSync(path.join(native, "bin"), { recursive: true });
    fs.mkdirSync(project);
    fs.mkdirSync(foreign);
    const nativeBinary = path.join(native, "bin", "ripr");
    fs.copyFileSync(binary, nativeBinary);
    fs.chmodSync(nativeBinary, 0o755);
    fs.writeFileSync(path.join(native, "package.json"), JSON.stringify({
      name: "@effortlessmetrics/ripr-linux-x64-gnu", version,
      riprNative: { schemaVersion: 1, product: "ripr", target: "x86_64-unknown-linux-gnu", executable: "bin/ripr" },
    }));
    fs.writeFileSync(path.join(project, "package.json"), '{"private":true}');
    const nativePack = JSON.parse(runNpm(["pack", "--json", "--ignore-scripts", "--pack-destination", root], native))[0];
    const launcherPack = JSON.parse(runNpm(["pack", "--json", "--ignore-scripts", "--pack-destination", root], launcherRoot))[0];
    runNpm(["install", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", "--package-lock=false", path.join(root, nativePack.filename), path.join(root, launcherPack.filename)], project);
    const installed = path.join(project, "node_modules", "@effortlessmetrics", "ripr-linux-x64-gnu", "bin", "ripr");
    const digest = (file) => crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
    assert.equal(digest(installed), digest(binary), "installed executable must equal selected binary");
    const request = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "package-test", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
    ].map((value) => JSON.stringify(value)).join("\n") + "\n";
    const options = { cwd: foreign, input: request, encoding: "utf8", timeout: 15000, maxBuffer: 1024 * 1024 };
    const direct = spawnSync(installed, ["mcp", "--stdio"], options);
    const launcher = path.join(project, "node_modules", "@effortlessmetrics", "ripr", "bin", "ripr.cjs");
    const wrapped = spawnSync(process.execPath, [launcher, "mcp", "--stdio"], options);
    assert.deepEqual(mcpReplies(wrapped), mcpReplies(direct));
    assert.throws(() => mcpReplies({ ...wrapped, stdout: `banner\n${wrapped.stdout}` }));
    fs.unlinkSync(installed);
    const marker = path.join(root, "ambient-called");
    fs.writeFileSync(path.join(foreign, "ripr"), `#!/bin/sh\ntouch '${marker}'\n`);
    fs.chmodSync(path.join(foreign, "ripr"), 0o755);
    const absent = spawnSync(process.execPath, [launcher, "mcp", "--stdio"], {
      ...options, env: { ...process.env, PATH: `${foreign}${path.delimiter}${process.env.PATH || ""}` },
    });
    assert.equal(absent.status, 1);
    assert.equal(absent.stdout, "");
    assert.match(absent.stderr, /does not contain/);
    assert.equal(fs.existsSync(marker), false, "missing payload must not fall back to PATH");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("the launcher bin remains executable in the checkout and packed package", (t) => {
  if (process.platform === "win32") {
    t.skip("Windows does not preserve POSIX executable mode bits");
    return;
  }

  const root = path.join(__dirname, "..");
  const bin = path.join(root, "bin", "ripr.cjs");
  assert.notEqual(fs.statSync(bin).mode & 0o111, 0, "bin/ripr.cjs must be executable in the checkout");

  const npmCli = process.env.npm_execpath;
  const command = npmCli ? process.execPath : "npm";
  const args = npmCli
    ? [npmCli, "pack", "--dry-run", "--json", "--ignore-scripts"]
    : ["pack", "--dry-run", "--json", "--ignore-scripts"];
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);

  const report = JSON.parse(result.stdout);
  const packedBin = report[0].files.find((entry) => entry.path === "bin/ripr.cjs");
  assert.ok(packedBin, "packed package must contain bin/ripr.cjs");
  assert.notEqual(packedBin.mode & 0o111, 0, "packed bin/ripr.cjs must be executable");
});
